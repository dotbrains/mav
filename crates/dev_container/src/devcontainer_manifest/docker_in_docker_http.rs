use http_client::AsyncBody;

#[path = "docker_in_docker_blob.rs"]
mod docker_in_docker_blob;

pub(crate) async fn response(path: &str) -> Option<http::Response<AsyncBody>> {
    if path == "/v2/devcontainers/features/docker-in-docker/manifests/2" {
        let response = r#"
        {
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.devcontainers",
                "digest": "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
                "size": 2
            },
            "layers": [
                {
                    "mediaType": "application/vnd.devcontainers.layer.v1+tar",
                    "digest": "sha256:bc7ab0d8d8339416e1491419ab9ffe931458d0130110f4b18351b0fa184e67d5",
                    "size": 59392,
                    "annotations": {
                        "org.opencontainers.image.title": "devcontainer-feature-docker-in-docker.tgz"
                    }
                }
            ],
            "annotations": {
                "dev.containers.metadata": "{\"id\":\"docker-in-docker\",\"version\":\"2.16.1\",\"name\":\"Docker (Docker-in-Docker)\",\"documentationURL\":\"https://github.com/devcontainers/features/tree/main/src/docker-in-docker\",\"description\":\"Create child containers *inside* a container, independent from the host's docker instance. Installs Docker extension in the container along with needed CLIs.\",\"options\":{\"version\":{\"type\":\"string\",\"proposals\":[\"latest\",\"none\",\"20.10\"],\"default\":\"latest\",\"description\":\"Select or enter a Docker/Moby Engine version. (Availability can vary by OS version.)\"},\"moby\":{\"type\":\"boolean\",\"default\":true,\"description\":\"Install OSS Moby build instead of Docker CE\"},\"mobyBuildxVersion\":{\"type\":\"string\",\"default\":\"latest\",\"description\":\"Install a specific version of moby-buildx when using Moby\"},\"dockerDashComposeVersion\":{\"type\":\"string\",\"enum\":[\"none\",\"v1\",\"v2\"],\"default\":\"v2\",\"description\":\"Default version of Docker Compose (v1, v2 or none)\"},\"azureDnsAutoDetection\":{\"type\":\"boolean\",\"default\":true,\"description\":\"Allow automatically setting the dockerd DNS server when the installation script detects it is running in Azure\"},\"dockerDefaultAddressPool\":{\"type\":\"string\",\"default\":\"\",\"proposals\":[],\"description\":\"Define default address pools for Docker networks. e.g. base=192.168.0.0/16,size=24\"},\"installDockerBuildx\":{\"type\":\"boolean\",\"default\":true,\"description\":\"Install Docker Buildx\"},\"installDockerComposeSwitch\":{\"type\":\"boolean\",\"default\":false,\"description\":\"Install Compose Switch (provided docker compose is available) which is a replacement to the Compose V1 docker-compose (python) executable. It translates the command line into Compose V2 docker compose then runs the latter.\"},\"disableIp6tables\":{\"type\":\"boolean\",\"default\":false,\"description\":\"Disable ip6tables (this option is only applicable for Docker versions 27 and greater)\"}},\"entrypoint\":\"/usr/local/share/docker-init.sh\",\"privileged\":true,\"containerEnv\":{\"DOCKER_BUILDKIT\":\"1\"},\"customizations\":{\"vscode\":{\"extensions\":[\"ms-azuretools.vscode-containers\"],\"settings\":{\"github.copilot.chat.codeGeneration.instructions\":[{\"text\":\"This dev container includes the Docker CLI (`docker`) pre-installed and available on the `PATH` for running and managing containers using a dedicated Docker daemon running inside the dev container.\"}]}}},\"mounts\":[{\"source\":\"dind-var-lib-docker-${devcontainerId}\",\"target\":\"/var/lib/docker\",\"type\":\"volume\"}],\"installsAfter\":[\"ghcr.io/devcontainers/features/common-utils\"]}",
                "com.github.package.type": "devcontainer_feature"
            }
        }
        "#;
        return Some(
            http::Response::builder()
                .status(200)
                .body(http_client::AsyncBody::from(response))
                .unwrap(),
        );
    }

    if path
        == "/v2/devcontainers/features/docker-in-docker/blobs/sha256:bc7ab0d8d8339416e1491419ab9ffe931458d0130110f4b18351b0fa184e67d5"
    {
        return Some(docker_in_docker_blob::response().await);
    }

    None
}
