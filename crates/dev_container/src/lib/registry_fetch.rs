use super::*;

pub(super) async fn get_ghcr_templates(
    client: Arc<dyn HttpClient>,
) -> Result<DevContainerTemplatesResponse, String> {
    let token = get_oci_token(
        ghcr_registry(),
        devcontainer_templates_repository(),
        &client,
    )
    .await?;
    let manifest = get_latest_oci_manifest(
        &token.token,
        ghcr_registry(),
        devcontainer_templates_repository(),
        &client,
        None,
    )
    .await?;

    let mut template_response: DevContainerTemplatesResponse = get_deserializable_oci_blob(
        &token.token,
        ghcr_registry(),
        devcontainer_templates_repository(),
        &manifest.layers[0].digest,
        &client,
    )
    .await?;

    for template in &mut template_response.templates {
        template.source_repository = Some(format!(
            "{}/{}",
            ghcr_registry(),
            devcontainer_templates_repository()
        ));
    }
    Ok(template_response)
}

pub(super) async fn get_ghcr_features(
    client: Arc<dyn HttpClient>,
) -> Result<DevContainerFeaturesResponse, String> {
    let token = get_oci_token(
        ghcr_registry(),
        devcontainer_templates_repository(),
        &client,
    )
    .await?;

    let manifest = get_latest_oci_manifest(
        &token.token,
        ghcr_registry(),
        devcontainer_features_repository(),
        &client,
        None,
    )
    .await?;

    let mut features_response: DevContainerFeaturesResponse = get_deserializable_oci_blob(
        &token.token,
        ghcr_registry(),
        devcontainer_features_repository(),
        &manifest.layers[0].digest,
        &client,
    )
    .await?;

    for feature in &mut features_response.features {
        feature.source_repository = Some(format!(
            "{}/{}",
            ghcr_registry(),
            devcontainer_features_repository()
        ));
    }
    Ok(features_response)
}
