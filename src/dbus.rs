// SPDX-License-Identifier: Apache-2.0

//! The session-bus interface: the `--toggle` CLI mode talks to the running
//! applet through it, and owning its name doubles as the singleton guard.
//!
//! Same shape as pop-os' `cosmic-applet-status-area`: serve a zbus interface
//! from the applet process, and offer a second mode of the same binary as the
//! client.

pub const BUS_NAME: &str = "io.github.enzofagundz.CosmicClipman";
pub const OBJECT_PATH: &str = "/io/github/enzofagundz/CosmicClipman";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcRequest {
    Toggle,
    Show,
    Hide,
}

#[derive(Clone)]
pub struct Service {
    sender: tokio::sync::mpsc::UnboundedSender<IpcRequest>,
}

#[zbus::interface(name = "io.github.enzofagundz.CosmicClipman")]
impl Service {
    fn toggle(&self) {
        let _ = self.sender.send(IpcRequest::Toggle);
    }

    fn show(&self) {
        let _ = self.sender.send(IpcRequest::Show);
    }

    fn hide(&self) {
        let _ = self.sender.send(IpcRequest::Hide);
    }
}

pub struct Running {
    #[allow(dead_code)]
    pub connection: zbus::Connection,
    /// Whether this process owns the bus name and therefore captures. A second
    /// applet instance runs without capture.
    pub owned: bool,
    pub requests: tokio::sync::mpsc::UnboundedReceiver<IpcRequest>,
}

pub async fn serve() -> zbus::Result<Running> {
    let connection = zbus::Connection::session().await?;
    let (sender, requests) = tokio::sync::mpsc::unbounded_channel();
    connection
        .object_server()
        .at(OBJECT_PATH, Service { sender })
        .await?;
    let owned = match connection
        .request_name_with_flags(BUS_NAME, zbus::fdo::RequestNameFlags::DO_NOT_QUEUE)
        .await
    {
        Ok(_) => true,
        Err(zbus::Error::NameTaken) => false,
        Err(err) => return Err(err),
    };
    Ok(Running {
        connection,
        owned,
        requests,
    })
}

/// Client side of the toggle CLI mode.
pub async fn request_toggle() -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|err| format!("no session bus: {err}"))?;
    let proxy = zbus::Proxy::new(&connection, BUS_NAME, OBJECT_PATH, BUS_NAME)
        .await
        .map_err(|err| format!("cannot reach the applet: {err}"))?;
    proxy
        .call_method("Toggle", &())
        .await
        .map_err(|err| format!("the applet did not accept the toggle: {err}"))?;
    Ok(())
}
