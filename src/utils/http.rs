use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs::File;
use std::io::Write;
use anyhow::Result;

#[derive(Debug, Deserialize, Serialize)]
pub struct HttpResponse<T> {
    pub code: u16,
    pub message: String,
    pub body: T,
}

impl<T> HttpResponse<T> {
    pub fn new(code: u16, message: String, body: T) -> Self {
        Self { code, message, body }
    }
}


pub async fn download_audio(url: &str, dest: &PathBuf) -> Result<PathBuf> {
    let response = reqwest::get(url).await?;
    let mut file = File::create(dest)?;
    file.write_all(&response.bytes().await?)?;
    Ok(dest.clone())
}