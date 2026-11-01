//! `reda mcp [--project p]`: an MCP server over stdio, in-process.
//!
//! One writer thread owns the engine ([`writer`]); the rmcp handler ([`server`]) sends it
//! every Command. Nothing is written to disk except by `project.save`. When the client
//! closes stdin, the server stops and warns on stderr if it discards unsaved changes.
//! Bridging to a running app comes with the control port.

mod search;
mod server;
mod writer;

#[cfg(test)]
mod tests;

use std::process::ExitCode;

use reda_engine::Engine;
use reda_engine::command::Signals;
use reda_engine::journal::Source;
use rmcp::ServiceExt;
use serde_json::json;
use tokio::io::{AsyncRead, AsyncWrite};

pub use server::Server;
pub use writer::Leftover;

/// The parsed arguments of `reda mcp`.
#[derive(Debug, PartialEq)]
pub struct McpArgs {
    pub project: Option<String>,
}

/// Parses everything after `mcp`. Errors are usage messages.
pub fn parse(args: &[&str]) -> Result<McpArgs, String> {
    match args {
        [] => Ok(McpArgs { project: None }),
        ["--project", project] => Ok(McpArgs {
            project: Some((*project).to_owned()),
        }),
        ["--project"] => Err("--project needs a path".into()),
        [other, ..] => Err(format!("unexpected argument '{other}'")),
    }
}

/// Serves MCP over stdio until the client closes stdin.
pub fn execute(args: McpArgs) -> ExitCode {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("start the async runtime");
    let (writer, thread, catalogue) =
        writer::spawn(|| Engine::builder().source(Source::Mcp).build());
    let served = runtime.block_on(async {
        if let Some(project) = &args.project {
            let opened = writer
                .run(
                    "project.open",
                    json!({ "path": project }),
                    Signals::default(),
                )
                .await;
            if let Err(err) = opened {
                return Err(serde_json::to_string_pretty(&err).expect("errors serialise"));
            }
        }
        serve(Server::new(writer, &catalogue), rmcp::transport::stdio())
            .await
            .map_err(|e| format!("reda: MCP transport failed: {e}"))
    });
    let leftover = thread.finish();
    // tokio's stdin reader may still be parked in a blocking read; don't wait for it.
    runtime.shutdown_background();
    if let Err(message) = served {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    if let Some(warning) = discard_warning(&leftover) {
        eprintln!("{warning}");
    }
    ExitCode::SUCCESS
}

/// Serves one client over `transport` until it disconnects. Consumes the server, so the
/// writer handle it holds is dropped when this returns.
pub async fn serve<R, W>(server: Server, transport: (R, W)) -> Result<(), String>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let running = server.serve(transport).await.map_err(|e| e.to_string())?;
    running.waiting().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// The stderr warning for changes discarded on exit, if there are any.
pub fn discard_warning(leftover: &Leftover) -> Option<String> {
    if leftover.dirty_files == 0 {
        return None;
    }
    let name = leftover.project.as_deref().unwrap_or("the Project");
    let files = match leftover.dirty_files {
        1 => "1 file".to_owned(),
        n => format!("{n} files"),
    };
    Some(format!(
        "reda: discarding unsaved changes to {name} ({files}); run project.save to keep them"
    ))
}
