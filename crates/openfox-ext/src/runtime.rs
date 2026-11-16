use crate::api::read_string;
use crate::{ExtError, Manifest};
use wasmtime::{Caller, Engine, Linker, Module, Store};

/// What the host tells the extension about the world it just entered.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExtContext {
    pub tab: i64,
    pub shield_blocked: i64,
}

/// What the extension tells the host it saw.
#[derive(Debug, Clone, Default)]
pub struct RunResult {
    pub logs: Vec<String>,
    pub report: i32,
}

/// The wasmtime wiring. One engine, many extensions; each run gets a fresh
/// store with a state cell the host functions mutate through `Caller`.
pub struct ExtRuntime {
    engine: Engine,
}

/// A compiled extension: bytes plus manifest, instantiated per run so
/// extensions stay stateless across navigations.
pub struct Ext {
    module: Module,
    manifest: Manifest,
}

impl ExtRuntime {
    pub fn new() -> ExtRuntime {
        ExtRuntime { engine: Engine::default() }
    }

    pub fn compile(&self, wasm: &[u8], manifest: Manifest) -> Result<Ext, ExtError> {
        Ok(Ext { module: Module::from_binary(&self.engine, wasm)?, manifest })
    }

    /// Instantiates and runs one extension for one navigation: the context
    /// string is written through the guest's own `alloc` export, then
    /// `on_navigation(tab)` fires and `report()` is read back. Host imports
    /// are `fox.log(ptr, len)` and `fox.shield_blocked()`.
    pub fn run(&self, ext: &Ext, ctx: ExtContext) -> Result<RunResult, ExtError> {
        #[derive(Default)]
        struct ExtState {
            logs: Vec<String>,
            shield_blocked: i64,
            allows_console: bool,
            allows_shield: bool,
        }

        let mut store = Store::new(&self.engine, ExtState::default());
        let mut linker: Linker<ExtState> = Linker::new(&self.engine);

        linker.func_wrap("fox", "log", |mut caller: Caller<'_, ExtState>, ptr: i32, len: i32| {
            let allows = caller.data().allows_console;
            let text = read_string(&mut caller, ptr, len);
            if allows {
                caller.data_mut().logs.push(text);
            }
        })?;
        linker.func_wrap("fox", "shield_blocked", |caller: Caller<'_, ExtState>| -> i64 {
            let state = caller.data();
            if state.allows_shield {
                state.shield_blocked
            } else {
                -1
            }
        })?;

        {
            let state = store.data_mut();
            state.shield_blocked = ctx.shield_blocked;
            state.allows_console = ext.manifest.allows(crate::Permission::Console);
            state.allows_shield = ext.manifest.allows(crate::Permission::Shield);
        }

        let instance = linker.instantiate(&mut store, &ext.module)?;

        let on_navigation = instance
            .get_typed_func::<i64, ()>(&mut store, "on_navigation")
            .map_err(|_| ExtError::MissingExport("on_navigation(tab)".into()))?;
        let report = instance
            .get_typed_func::<(), i32>(&mut store, "report")
            .map_err(|_| ExtError::MissingExport("report()".into()))?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|_| ExtError::MissingExport("alloc(len)".into()))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| ExtError::MissingExport("memory".into()))?;

        // Hand the context to the guest through its own allocator.
        let payload = format!("tab:{};blocked:{}", ctx.tab, ctx.shield_blocked);
        let ptr = alloc.call(&mut store, payload.len() as i32)?;
        memory.write(&mut store, ptr as u32 as usize, payload.as_bytes())?;

        on_navigation.call(&mut store, ctx.tab)?;
        let report_value = report.call(&mut store, ())?;

        let state = store.into_data();
        Ok(RunResult { logs: state.logs, report: report_value })
    }
}

impl Default for ExtRuntime {
    fn default() -> Self {
        ExtRuntime::new()
    }
}
