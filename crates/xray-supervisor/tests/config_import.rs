use std::sync::{Arc, Mutex};
use windows_integration::ProxyBackend;
use xray_model::*;
use xray_supervisor::Supervisor;

#[derive(Clone, Default)]
struct FakeProxy(Arc<Mutex<ProxySnapshot>>);
impl ProxyBackend for FakeProxy {
    fn read(&self) -> Result<ProxySnapshot> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn write(&mut self, value: &ProxySnapshot) -> Result<()> {
        *self.0.lock().unwrap() = value.clone();
        Ok(())
    }
}

#[tokio::test]
async fn incomplete_import_preserves_source_and_replay_but_does_not_start_core() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = Supervisor::with_backend(tmp.path(), FakeProxy::default())
        .await
        .unwrap();
    let json = "{\n \"outbounds\": [{\"tag\":\"missing-protocol\",\"extra\":{\"retain\":true}}],\n \"routing\":{\"rules\":[{\"outboundTag\":\"api\"}]}\n}";
    let create = Request::new(
        Command::CreateProfile {
            profile_id: "partial".into(),
            json: json.into(),
        },
        None,
    );
    let result = s.handle(create.clone()).await.result.unwrap();
    assert_eq!(result["json"], json);
    assert_eq!(s.handle(create).await.result.unwrap(), result);
    assert_eq!(s.store.profile("partial").unwrap().json, json);
    let failure = s
        .handle(Request::new(
            Command::ValidateDraft {
                profile_id: "partial".into(),
                expected_revision: hash(json.as_bytes()),
                json: json.into(),
                core_path: tmp.path().join("not-used"),
                asset_root: tmp.path().into(),

                allow_dynamic: false,
            },
            None,
        ))
        .await
        .result
        .unwrap_err();
    assert_eq!(failure.code, "STATIC_VALIDATION_FAILED");
    assert!(failure.message.contains("/outbounds/0/protocol"));
    let status: Status = serde_json::from_value(
        s.handle(Request::new(Command::GetStatus, None))
            .await
            .result
            .unwrap(),
    )
    .unwrap();
    assert!(status.generation.is_none());
    assert_eq!(s.store.profile("partial").unwrap().json, json);
    assert!(!status.recovery_required);
}

#[tokio::test]
#[ignore = "requires XRAY_TEST_CORE; optional XRAY_TEST_CONFIG is read only; no Core start or system proxy writes"]
async fn official_api_configuration_can_be_imported_and_checked_without_starting() {
    let core =
        std::path::PathBuf::from(std::env::var_os("XRAY_TEST_CORE").expect("XRAY_TEST_CORE"));
    let json = if let Some(path) = std::env::var_os("XRAY_TEST_CONFIG") {
        std::fs::read_to_string(path).unwrap()
    } else {
        serde_json::json!({
            "api":{"tag":"api","services":["StatsService"]}, "stats":{},
            "inbounds":[{"tag":"api","listen":"127.0.0.1","port":10085,"protocol":"dokodemo-door","settings":{"address":"127.0.0.1"}}],
            "outbounds":[{"tag":"direct","protocol":"freedom"}],
            "routing":{"rules":[{"type":"field","inboundTag":["api"],"outboundTag":"api"}]}
        }).to_string()
    };
    assert!(xray_config::parse_config(json.as_bytes()).is_ok());
    let tmp = tempfile::tempdir().unwrap();
    let mut s = Supervisor::with_backend(tmp.path(), FakeProxy::default())
        .await
        .unwrap();
    let imported = s
        .handle(Request::new(
            Command::CreateProfile {
                profile_id: "api-config".into(),
                json: json.clone(),
            },
            None,
        ))
        .await
        .result;
    assert!(imported.is_ok(), "Native import must succeed");
    let checked = s
        .handle(Request::new(
            Command::ValidateDraft {
                profile_id: "api-config".into(),
                expected_revision: hash(json.as_bytes()),
                json,
                core_path: core.clone(),
                asset_root: core.parent().unwrap().into(),

                allow_dynamic: false,
            },
            None,
        ))
        .await
        .result;
    // Do not print native configuration or error text: external files may contain secrets.
    assert!(
        checked.is_ok(),
        "Official Core must accept the imported configuration"
    );
    let status: Status = serde_json::from_value(
        s.handle(Request::new(Command::GetStatus, None))
            .await
            .result
            .unwrap(),
    )
    .unwrap();
    assert!(status.generation.is_none());
}

#[tokio::test]
async fn update_profile_persists_valid_json_enforces_revision_and_keeps_runtime_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = Supervisor::with_backend(tmp.path(), FakeProxy::default())
        .await
        .unwrap();
    let initial = r#"{"outbounds":[{"tag":"direct","protocol":"freedom"}]}"#;
    s.handle(Request::new(
        Command::CreateProfile {
            profile_id: "editable".into(),
            json: initial.into(),
        },
        None,
    ))
    .await
    .result
    .unwrap();

    let updated = r#"{"outbounds":[{"tag":"node-1","protocol":"vless"}]}"#;
    let update = Request::new(
        Command::UpdateProfile {
            profile_id: "editable".into(),
            expected_revision: hash(initial.as_bytes()),
            json: updated.into(),
        },
        None,
    );
    let saved: Profile =
        serde_json::from_value(s.handle(update.clone()).await.result.unwrap()).unwrap();
    assert_eq!(saved.json, updated);
    assert_eq!(saved.revision, hash(updated.as_bytes()));
    assert_eq!(s.store.profile("editable").unwrap().json, updated);
    // Replay returns the same outcome via durable operation evidence.
    let replayed: Profile = serde_json::from_value(s.handle(update).await.result.unwrap()).unwrap();
    assert_eq!(replayed.revision, saved.revision);

    // Stale expected_revision is rejected without overwriting.
    let conflict = s
        .handle(Request::new(
            Command::UpdateProfile {
                profile_id: "editable".into(),
                expected_revision: hash(initial.as_bytes()),
                json: r#"{"stale":true}"#.into(),
            },
            None,
        ))
        .await
        .result
        .unwrap_err();
    assert_eq!(conflict.code, "ASSET_REVISION_CONFLICT");
    assert_eq!(s.store.profile("editable").unwrap().json, updated);

    // Malformed JSON is rejected without touching stored profile.
    let invalid = s
        .handle(Request::new(
            Command::UpdateProfile {
                profile_id: "editable".into(),
                expected_revision: saved.revision.clone(),
                json: "{not-json".into(),
            },
            None,
        ))
        .await
        .result
        .unwrap_err();
    assert_eq!(invalid.code, "CONFIG_PARSE_FAILED");
    assert_eq!(s.store.profile("editable").unwrap().json, updated);
}
