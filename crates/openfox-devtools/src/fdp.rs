use crate::{console::Console, network::NetworkLog, profiler::FrameProfiler};
use serde_json::Value;

/// FDP — the Fox Devtools Protocol. CDP-shaped, scoped to what OpenFox
/// actually implements. A devtools client talks to the browser through the
/// same buffers the in-app panels read; there is no second source of truth.
#[derive(Debug, thiserror::Error)]
pub enum FdpError {
    #[error("message not parseable")]
    Parse(#[from] serde_json::Error),
    #[error("method {0} is not an FDP method")]
    UnknownMethod(String),
    #[error("domain {0} needs {1}")]
    MissingParameter(String, &'static str),
}

pub fn handle(devtools: &mut crate::Devtools, message: Value) -> Result<Value, FdpError> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| FdpError::UnknownMethod(String::new()))?;

    let (domain, rest) = method
        .strip_prefix("FDP.")
        .and_then(|m| m.split_once('.'))
        .ok_or_else(|| FdpError::UnknownMethod(method.to_owned()))?;

    match domain {
        "Network" => network_method(&mut devtools.network, rest),
        "Console" => console_method(&mut devtools.console, rest),
        "Performance" => performance_method(&mut devtools.profiler, rest, message.get("params")),
        _ => Err(FdpError::UnknownMethod(method.to_owned())),
    }
}

fn network_method(network: &mut NetworkLog, method: &str) -> Result<Value, FdpError> {
    match method {
        "enable" | "disable" => Ok(Value::Bool(true)),
        "waterfall" => {
            let bars = network.waterfall();
            let value = serde_json::json!({
                "bars": bars
                    .into_iter()
                    .map(|bar| {
                        serde_json::json!({
                            "url": bar.url,
                            "startMs": bar.start_ms,
                            "totalMs": bar.total_ms,
                            "blocked": bar.blocked,
                        })
                    })
                    .collect::<Vec<_>>(),
            });
            Ok(value)
        }
        "log" => Ok(network.to_json(None)),
        "clear" => {
            network.clear();
            Ok(Value::Bool(true))
        }
        unknown => Err(FdpError::UnknownMethod(format!("FDP.Network.{unknown}"))),
    }
}

fn console_method(console: &mut Console, method: &str) -> Result<Value, FdpError> {
    match method {
        "enable" | "disable" => Ok(Value::Bool(true)),
        "log" => Ok(console.to_json(None)),
        "clear" => {
            console.clear();
            Ok(Value::Bool(true))
        }
        unknown => Err(FdpError::UnknownMethod(format!("FDP.Console.{unknown}"))),
    }
}

fn performance_method(
    profiler: &mut FrameProfiler,
    method: &str,
    params: Option<&Value>,
) -> Result<Value, FdpError> {
    match method {
        "enable" | "disable" => Ok(Value::Bool(true)),
        "frame" => {
            let ms = params
                .and_then(|p| p.get("ms"))
                .and_then(Value::as_f64)
                .ok_or_else(|| FdpError::MissingParameter("FDP.Performance.frame".into(), "ms"))?;
            profiler.frame(ms as f32);
            Ok(Value::Bool(true))
        }
        "summary" => Ok(profiler.to_json()),
        unknown => Err(FdpError::UnknownMethod(format!("FDP.Performance.{unknown}"))),
    }
}
