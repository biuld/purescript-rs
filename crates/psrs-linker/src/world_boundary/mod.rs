//! WIT boundary encoding kept separate from private raw Core instance assembly.
use crate::{CheckedLinkPlan, LinkErrors, LinkStage, ResolvedWorldContext};
use std::borrow::Cow;
use wasm_encoder::reencode::{Error, Reencode, ReencodeComponent};
use wasm_encoder::{
    CodeSection, Component, CustomSection, Function, FunctionSection, Instruction, Module,
    RawSection,
};
use wasmparser::{Parser, Payload, TypeRef};

const MARKER: &str = "psrs-linker:canonical-boundary";

pub(super) fn encode(
    context: &ResolvedWorldContext,
    plan: &CheckedLinkPlan,
    application: &[u8],
) -> Result<Vec<u8>, LinkErrors> {
    fn error(message: impl Into<String>) -> LinkErrors {
        LinkErrors::plain(LinkStage::Compose, message)
    }
    let libraries = crate::core_assembly::canonical_libraries(plan);
    let prepared = crate::core_assembly::prepare_core(plan, application, &libraries)?;
    let application_module = prepared.application_module;
    let providers = prepared.providers;
    let builder = prepared.component;
    let modules = builder.core_module_count();
    let instances = builder.core_instance_count();
    let prefix = builder.finish();
    let mut visible = plan.external_world().to_vec();
    visible.extend(libraries.iter().cloned());
    let mut facade = boundary_module(application, &visible).map_err(error)?;
    let world = context
        .composition_world()
        .ok_or_else(|| error("canonical boundary requires a resolved world"))?;
    wit_component::embed_component_metadata(
        &mut facade,
        context.resolve(),
        world,
        wit_component::StringEncoding::UTF8,
    )
    .map_err(|e| error(format!("{e:#}")))?;
    let mut encoder = wit_component::ComponentEncoder::default()
        .module(&facade)
        .map_err(|e| error(format!("{e:#}")))?;
    for artifact in plan
        .artifacts()
        .iter()
        .filter(|artifact| libraries.contains(&artifact.module_name))
    {
        encoder = encoder
            .library(
                &artifact.module_name,
                &artifact.bytes,
                wit_component::LibraryInfo {
                    instantiate_after_shims: artifact.instantiate_after_shims,
                    arguments: Vec::new(),
                },
            )
            .map_err(|e| error(format!("{e:#}")))?;
    }
    let boundary = encoder
        .validate(true)
        .encode()
        .map_err(|e| error(format!("{e:#}")))?;
    let mut component = Component::new();
    CopyModules
        .parse_component(&mut component, Parser::new(0), &prefix)
        .map_err(|e| error(format!("{e:#}")))?;
    let mut attach = Attach {
        modules,
        instances,
        providers,
        application_module,
        depth: 0,
        module_seen: false,
        instance_seen: false,
        source_instances: 0,
        source_application: None,
    };
    attach
        .parse_component(&mut component, Parser::new(0), &boundary)
        .map_err(|e| error(format!("{e:#}")))?;
    if !attach.module_seen || !attach.instance_seen {
        return Err(error(
            "canonical boundary did not identify its application instance",
        ));
    }
    Ok(component.finish())
}

mod facade;
use facade::boundary_module;

struct CopyModules;
impl Reencode for CopyModules {
    type Error = String;
}
impl ReencodeComponent for CopyModules {
    fn parse_component_submodule(
        &mut self,
        component: &mut Component,
        _: Parser,
        bytes: &[u8],
    ) -> Result<(), Error<String>> {
        component.section(&RawSection { id: 1, data: bytes });
        Ok(())
    }
}

struct Attach {
    modules: u32,
    instances: u32,
    providers: std::collections::BTreeMap<String, u32>,
    application_module: u32,
    depth: u32,
    source_instances: u32,
    source_application: Option<u32>,
    module_seen: bool,
    instance_seen: bool,
}
impl Reencode for Attach {
    type Error = String;
}
impl ReencodeComponent for Attach {
    fn push_depth(&mut self) {
        self.depth += 1;
    }
    fn pop_depth(&mut self) {
        self.depth -= 1;
    }
    fn module_index(&mut self, index: u32) -> u32 {
        if self.depth == 0 {
            self.root_module(index)
        } else {
            index
        }
    }
    fn outer_module_index(&mut self, count: u32, index: u32) -> u32 {
        if count == self.depth {
            self.root_module(index)
        } else {
            index
        }
    }
    fn instance_index(&mut self, index: u32) -> u32 {
        if self.depth != 0 {
            return index;
        }
        index + self.instances
    }
    fn parse_component_submodule(
        &mut self,
        component: &mut Component,
        parser: Parser,
        bytes: &[u8],
    ) -> Result<(), Error<String>> {
        let marker = Parser::new(0).parse_all(bytes).any(|payload| matches!(payload, Ok(Payload::CustomSection(section)) if section.name() == MARKER && section.data() == b"1"));
        if self.depth == 0 && !self.module_seen {
            if !marker {
                return Err(Error::UserError(
                    "canonical application module lacks its identity marker".into(),
                ));
            }
            self.module_seen = true;
            return Ok(());
        }
        if marker {
            return Err(Error::UserError(
                "canonical application module identity is repeated".into(),
            ));
        }
        wasm_encoder::reencode::component_utils::parse_component_submodule(
            self, component, parser, bytes,
        )
    }
    fn parse_instance(
        &mut self,
        instances: &mut wasm_encoder::InstanceSection,
        instance: wasmparser::Instance<'_>,
    ) -> Result<(), Error<String>> {
        if self.depth == 0 {
            let index = self.source_instances;
            self.source_instances += 1;
            if matches!(
                instance,
                wasmparser::Instance::Instantiate {
                    module_index: 0,
                    ..
                }
            ) {
                if self.instance_seen {
                    return Err(Error::UserError(
                        "canonical application instance is repeated".into(),
                    ));
                }
                let wasmparser::Instance::Instantiate { args, .. } = instance else {
                    unreachable!()
                };
                let mut actual = args
                    .iter()
                    .map(|arg| {
                        if self.providers.contains_key(arg.name) {
                            return Err(Error::UserError(
                                "canonical host and raw provider namespaces conflict".into(),
                            ));
                        }
                        Ok((
                            arg.name,
                            wasm_encoder::ModuleArg::Instance(self.instance_index(arg.index)),
                        ))
                    })
                    .collect::<Result<Vec<_>, Error<String>>>()?;
                actual.extend(self.providers.iter().map(|(name, index)| {
                    (name.as_str(), wasm_encoder::ModuleArg::Instance(*index))
                }));
                instances.instantiate(self.application_module, actual);
                self.instance_seen = true;
                self.source_application = Some(index);
                return Ok(());
            }
        }
        wasm_encoder::reencode::component_utils::parse_instance(self, instances, instance)
    }
}

impl Attach {
    fn root_module(&self, index: u32) -> u32 {
        if index == 0 {
            self.application_module
        } else {
            index + self.modules - 1
        }
    }
}

#[cfg(test)]
mod tests;
