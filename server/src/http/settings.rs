use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};

use crate::{
    app::AppState,
    services::dev_selftest_allowlist::{
        summary_for_state, update_allowlist, AllowlistUpdateRequest, AllowlistUpdateResponse,
        DevSelftestConfigSummary,
    },
    services::dev_selftest_profiles::{
        get_profiles, upsert_profile, DevSelftestProfilesResponse, ProfileKind, ProfileUpsertBody,
        ProfileUpsertResponse,
    },
    support::error::AppError,
};

pub async fn get_dev_selftest_git_allowlist(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DevSelftestConfigSummary>, AppError> {
    Ok(Json(summary_for_state(&state)))
}

pub async fn put_dev_selftest_git_allowlist(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AllowlistUpdateRequest>,
) -> Result<Json<AllowlistUpdateResponse>, AppError> {
    update_allowlist(&state, request).await.map(Json)
}

pub async fn get_dev_selftest_profiles(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DevSelftestProfilesResponse>, AppError> {
    Ok(Json(get_profiles(&state)))
}

pub async fn put_dev_selftest_profile(
    State(state): State<Arc<AppState>>,
    Path((kind, id)): Path<(String, String)>,
    Json(body): Json<ProfileUpsertBody>,
) -> Result<Json<ProfileUpsertResponse>, AppError> {
    let kind = ProfileKind::parse(&kind)?;
    upsert_profile(&state, body.into_request(kind, id))
        .await
        .map(Json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use std::{collections::BTreeMap, path::PathBuf};
    use tower::ServiceExt;

    use crate::support::config::{
        AppConfig, AuthSettings, DevSelftestGitRepo, DevSelftestGitSettings, DevSelftestSettings,
        LogAnalyzerSettings, McpSettings, RemoteExecutionSettings, ServerSettings, StorageSettings,
        ToolsSettings,
    };

    #[tokio::test]
    async fn settings_api_reads_allowlist_and_rejects_missing_consent() {
        let root = std::env::temp_dir().join(format!(
            "logagent-settings-api-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let config = Arc::new(AppConfig {
            server: ServerSettings {
                bind: "127.0.0.1:0".to_string(),
                public_base_url: "http://127.0.0.1:0".to_string(),
                max_concurrent_tasks: 1,
                max_input_chars: 1024,
            },
            auth: AuthSettings {
                api_keys: vec!["test-key".to_string()],
            },
            storage: StorageSettings {
                data_dir: root.join("data"),
                max_upload_bytes: 1024,
                max_chunk_bytes: 1024,
            },
            log_analyzer: LogAnalyzerSettings {
                keywords: Vec::new(),
                max_matches: 10,
            },
            tools: ToolsSettings {
                tools: BTreeMap::new(),
            },
            remote_execution: RemoteExecutionSettings::default(),
            mcp: McpSettings::default(),
            dev_selftest: DevSelftestSettings {
                enabled: true,
                git: DevSelftestGitSettings {
                    enabled: true,
                    binary: PathBuf::from("/usr/bin/git"),
                    repos: vec![DevSelftestGitRepo {
                        url: "https://example.test/project.git".to_string(),
                        refs: vec!["main".to_string()],
                    }],
                },
                ..DevSelftestSettings::default()
            },
        });
        config.prepare_dirs().unwrap();
        let state = AppState::new(config).unwrap();
        let app = crate::http::router(state.clone()).with_state(state);

        let response = app
            .clone()
            .oneshot(
                Request::get("/api/settings/dev-selftest/git-allowlist")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["defaultGitRef"], "main");
        assert_eq!(
            body["gitRepos"][0]["url"],
            "https://example.test/project.git"
        );

        let response = app
            .oneshot(
                Request::put("/api/settings/dev-selftest/git-allowlist")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"repoUrl":"https://example.test/project.git","gitRef":"feature"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body["error"]
            .as_str()
            .unwrap()
            .contains("confirmedUserConsent"));

        let _ = std::fs::remove_dir_all(root);
    }
}
