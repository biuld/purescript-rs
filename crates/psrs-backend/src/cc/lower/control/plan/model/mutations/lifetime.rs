use super::*;

#[test]
fn unused_private_allocations_are_not_hidden_by_fresh_id_normalization() {
    for allocation in [
        Op::NewStatus,
        Op::FreshExtent { parent: None },
        Op::EmptySequence,
    ] {
        let f = Fixture::new();
        detects(f, Program::Start, vec![], |plan| {
            let Exec::Steps { ops, .. } = plan else {
                unreachable!()
            };
            ops.push(allocation);
        });
    }
}

#[test]
fn owned_phases_remain_ordered_even_when_the_final_status_matches() {
    let mut f = Fixture::new();
    let computation = f.heap.node(Node::OwnedResume(f.owner, f.answer));
    let args = f.args(computation, f.consumer, f.root);
    detects(f, Program::Executor, args, |plan| {
        assert!(edit(arm(plan, C::OwnedResume), &mut |op| {
            if let Op::SetStatus { value, .. } = op
                && *value == 1
            {
                *value = 2;
                return true;
            }
            false
        }));
    });
}

#[test]
fn ownership_transfer_releases_the_callback_instead_of_retaining_it() {
    let mut f = Fixture::new();
    let computation = f.heap.node(Node::OwnedResume(f.owner, f.answer));
    let args = f.args(computation, f.consumer, f.root);
    detects(f, Program::Executor, args, |plan| {
        assert!(edit(arm(plan, C::OwnedResume), &mut |op| {
            if let Op::WriteCallback { value, .. } = op {
                *value = Operand::Temp(6);
                return true;
            }
            false
        }));
    });
}

#[test]
fn cancellation_marks_cancelled_instead_of_consumed() {
    let f = Fixture::new();
    let args = vec![f.owner];
    detects(f, Program::Cancel, args, |plan| {
        assert!(edit(plan, &mut |op| {
            if let Op::SetStatus { value, .. } = op {
                *value = 2;
                return true;
            }
            false
        }));
    });
}

#[test]
fn checking_an_expired_extent_cannot_reopen_it() {
    let mut f = Fixture::new();
    f.expire(f.checkpoint);
    let args = vec![f.checkpoint];
    detects(f, Program::Check, args, |plan| {
        assert!(edit(plan, &mut |op| {
            if let Op::GuardPending { cell } = op {
                *op = Op::SetStatus {
                    cell: cell.clone(),
                    value: 0,
                };
                return true;
            }
            false
        }));
    });
}

#[test]
fn close_cannot_leave_an_extent_live() {
    let f = Fixture::new();
    let args = vec![f.checkpoint];
    detects(f, Program::Close, args, |plan| {
        assert!(edit(plan, &mut |op| {
            if let Op::SetStatus { value, .. } = op {
                *value = 0;
                return true;
            }
            false
        }));
    });
}

#[test]
fn raw_resume_checks_its_saved_checkpoint_before_publishing_a_frame() {
    let mut f = Fixture::new();
    f.expire(f.checkpoint);
    let args = vec![f.raw, f.answer];
    detects(f, Program::RawResume, args, |plan| {
        assert!(edit(plan, &mut |op| {
            if let Op::CheckBoundary { extent } = op {
                *op = Op::FreshExtent {
                    parent: Some(extent.clone()),
                };
                return true;
            }
            false
        }));
    });
}

#[test]
fn run_cannot_omit_normal_root_cleanup_or_publish_an_unrelated_result() {
    for wrong_result in [false, true] {
        let f = Fixture::new();
        let args = vec![f.state, f.handler, f.next];
        detects(f, Program::Run, args, |plan| {
            let Exec::Steps { ops, next } = plan else {
                unreachable!()
            };
            if wrong_result {
                *next = Box::new(Exec::Return {
                    value: Operand::Param(0),
                });
            } else {
                let close = ops
                    .iter_mut()
                    .find(|op| matches!(op, Op::CloseBoundary { .. }))
                    .unwrap();
                let Op::CloseBoundary { extent } = close else {
                    unreachable!()
                };
                *close = Op::CheckBoundary {
                    extent: extent.clone(),
                };
            }
        });
    }
}
