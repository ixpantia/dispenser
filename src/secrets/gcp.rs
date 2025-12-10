use google_cloud_secretmanager_v1::client::SecretManagerService;

pub async fn fetch_secret(name: &str, version: &str) -> String {
    let result: Result<String, Box<dyn std::error::Error + Send + Sync>> = async {
        let client = SecretManagerService::builder().build().await?;

        let response = client
            .access_secret_version()
            .set_name(format!("{name}/versions/{version}"))
            .send()
            .await?;

        let payload = response.payload.ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Secret payload is empty or missing",
            ))
        })?;

        let secret_bytes = payload.data;
        let secret_str = std::str::from_utf8(&secret_bytes)?;

        Ok(secret_str.to_string())
    }
    .await;

    match result {
        Ok(secret) => secret,
        Err(e) => {
            // Log the error and return an empty string to make the function "infalible"
            log::error!(
                "Error fetching secret '{}/versions/{}': {}",
                name,
                version,
                e
            );
            "".to_string()
        }
    }
}
