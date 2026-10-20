mod controller;
mod presence;
mod provision;
mod policy;

use std::collections::BTreeSet;
use std::time::Duration;

use beacon::{Beacon, BeaconConfig, Connectability};
use link::{Link, LinkConfig, Status};
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};
use uuid::Uuid;

use controller::Controller;
use presence::Presence;
use provision::{NmcliConnector, provision};

const DEBOUNCE_WINDOW: Duration = Duration::from_secs(3);
const SERVICE_UUID: &str = "6a05c021-50be-4c40-ae51-a0676f28b589";
const CREDENTIALS_UUID: &str = "6a05c022-50be-4c40-ae51-a0676f28b589";
const STATUS_UUID: &str = "6a05c023-50be-4c40-ae51-a0676f28b589";
const BEACON_NAME: &str = "Dell G3";
/// Placeholder until the rotating auth token exists.
const TEST_COMPANY_ID: u16 = 0xFFFF;
const PLACEHOLDER_PAYLOAD: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let beacon = Beacon::new(BeaconConfig {
        local_name: BEACON_NAME.to_string(),
        uuids: BTreeSet::from([Uuid::parse_str(SERVICE_UUID)?]),
        connection_type: Connectability::Connectable,
        company_id: TEST_COMPANY_ID,
        payload: PLACEHOLDER_PAYLOAD.to_vec(),
    });
    let (credentials_tx, mut credentials_rx) = mpsc::channel(1);
    let (status_tx, status_rx) = watch::channel(Status::Idle);
    let link = Link::new(
        LinkConfig {
            service_uuid: Uuid::parse_str(SERVICE_UUID)?,
            credentials_uuid: Uuid::parse_str(CREDENTIALS_UUID)?,
            status_uuid: Uuid::parse_str(STATUS_UUID)?,
        },
        credentials_tx,
        status_rx,
    );
    let mut controller = Controller::new(Presence { beacon, link });

    let mut updates = netwatch::watch(DEBOUNCE_WINDOW).await?;

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            update = updates.recv() => {
                let Some(update) = update else {
                    warn!("connectivity watcher stopped");
                    break;
                };
                match &update {
                    Ok(state) => info!(?state, "connectivity changed"),
                    Err(e) => warn!(error = %e, "connectivity error"),
                }
                if let Some(wanted) = policy::beacon_wanted(&update) {
                    controller.set_wanted(wanted).await;
                }
            }
            Some(creds) = credentials_rx.recv() => {
                // Awaited inline: connectivity updates queue in their channel meanwhile.
                provision(&NmcliConnector, &creds, &status_tx).await;
            }
            result = &mut shutdown => {
                result?;
                info!("shutting down");
                break;
            }
        }
    }

    Ok(())
}
