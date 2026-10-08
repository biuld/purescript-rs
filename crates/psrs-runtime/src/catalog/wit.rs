//! Pinned interface sources and the default application world.

/// One pinned WIT source file: its catalog path and exact contents.
#[derive(Clone, Copy, Debug)]
pub struct WitSource {
    pub path: &'static str,
    pub contents: &'static str,
}

/// The default world the command artifact implements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldIdentity {
    pub package_namespace: &'static str,
    pub package_name: &'static str,
    pub world: &'static str,
}

/// The `psrs:app/command` default world.
pub const DEFAULT_WORLD: WorldIdentity = WorldIdentity {
    package_namespace: "psrs",
    package_name: "app",
    world: "command",
};

/// Vendored WASI 0.2.12 WIT in dependency order.
pub const WASI_WIT: &[WitSource] = &[
    WitSource {
        path: "wasi/io.wit",
        contents: include_str!("../../wit/deps/io.wit"),
    },
    WitSource {
        path: "wasi/clocks.wit",
        contents: include_str!("../../wit/deps/clocks.wit"),
    },
    WitSource {
        path: "wasi/random.wit",
        contents: include_str!("../../wit/deps/random.wit"),
    },
    WitSource {
        path: "wasi/filesystem.wit",
        contents: include_str!("../../wit/deps/filesystem.wit"),
    },
    WitSource {
        path: "wasi/sockets.wit",
        contents: include_str!("../../wit/deps/sockets.wit"),
    },
    WitSource {
        path: "wasi/cli.wit",
        contents: include_str!("../../wit/deps/cli.wit"),
    },
];

/// The application world source.
pub const APP_WIT: WitSource = WitSource {
    path: "psrs-app.wit",
    contents: include_str!("../../wit/psrs-app.wit"),
};
