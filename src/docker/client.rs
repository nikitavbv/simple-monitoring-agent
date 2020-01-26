use hyper::{Method, Request, Body, Client};
use hyperlocal::{UnixConnector, Uri, UnixClientExt};
use futures::{StreamExt, TryStreamExt};
use custom_error::custom_error;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::path::Path;
use std::collections::HashMap;

custom_error! {pub DockerClientError
    RequestToDockerFailed = "request to docker failed"
}

impl From<hyper::error::Error> for DockerClientError {
    fn from(err: hyper::error::Error) -> Self {
        DockerClientError::RequestToDockerFailed
    }
}

impl From<http::Error> for DockerClientError {
    fn from(err: http::Error) -> Self {
        DockerClientError::RequestToDockerFailed
    }
}

impl From<serde_json::error::Error> for DockerClientError {
    fn from(err: serde_json::error::Error) -> Self {
        DockerClientError::RequestToDockerFailed
    }
}

#[derive(Deserialize, Debug)]
pub struct Container {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "State")]
    pub state: String,
}

#[derive(Deserialize, Debug)]
pub struct ContainerStats {
    pub name: String,
    pub cpu_stats: CPUStats,
    pub memory_stats: MemoryStats,
    pub networks: HashMap<String, NetworkStat>
}

#[derive(Deserialize, Debug)]
pub struct CPUStats {
    pub cpu_usage: CPUUsage,
    pub system_cpu_usage: u128
}

#[derive(Deserialize, Debug)]
pub struct CPUUsage {
    pub total_usage: u128
}

#[derive(Deserialize, Debug)]
pub struct MemoryStats {
    pub usage: u64,
    pub stats: MemoryUsageStats
}

#[derive(Deserialize, Debug)]
pub struct MemoryUsageStats {
    pub cache: u64
}

#[derive(Deserialize, Debug)]
pub struct NetworkStat {
    pub rx_bytes: u64,
    pub tx_bytes: u64
}

pub async fn containers() -> Result<Vec<Container>, DockerClientError> {
    request(Method::GET, "/containers/json", "".to_string()).await
}

pub async fn stats(container_id: String) -> Result<ContainerStats, DockerClientError> {
    request(Method::GET, &format!("/containers/{}/stats?stream=false", container_id), "".to_string()).await
}

async fn request<T: DeserializeOwned>(method: Method, url: &str, body: String) -> Result<T, DockerClientError> {
    let client = Client::unix();

    let req = Request::builder()
        .uri(Uri::new(Path::new("/var/run/docker.sock"), url))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .method(method)
        .body(Body::from(body))?;

    let res = client.request(req).await?.into_body();

    let bytes = res.try_fold(Vec::default(), |mut buf, bytes| async {
        buf.extend(bytes);
        Ok(buf)
    }).await?;

    Ok(serde_json::from_slice(&bytes)?)
}