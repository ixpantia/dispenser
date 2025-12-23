use google_cloud_secretmanager_v1::client::SecretManagerService;

use crate::service::vars::ServiceConfigError;

pub async fn fetch_secret(name: &str, version: &str) -> Result<String, ServiceConfigError> {
    let client = SecretManagerService::builder().build().await?;

    let response = client
        .access_secret_version()
        .set_name(format!("{name}/versions/{version}"))
        .send()
        .await?;

    let payload = response.payload.unwrap_or_default();

    let secret_bytes = payload.data;
    let secret_str = std::str::from_utf8(&secret_bytes)?;

    Ok(secret_str.to_string())
}
