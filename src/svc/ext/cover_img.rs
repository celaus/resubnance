use std::fmt;

use reqwest::Client;
use url::Url;

use crate::{
    config,
    svc::{
        SingletonService,
        ext::{
            ExtServiceCommand,
            ExtServiceResponse,
            ExternalServiceCommandRx,
        },
    },
};

pub struct CoverImageDisplayService {
    url: Url,
    client: Client,
    command_channel: ExternalServiceCommandRx,
}

impl fmt::Debug for CoverImageDisplayService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoverImageDisplayService").finish()
    }
}

impl CoverImageDisplayService {
    #[tracing::instrument]
    pub fn new(conf: config::ExternalServices, cmds: ExternalServiceCommandRx) -> Self {
        if let config::ExternalServices::CoverImageUploader { url } = conf {
            let client = Client::builder()
                .build()
                .expect("Failed to build HTTP client");
            CoverImageDisplayService {
                url,
                client,
                command_channel: cmds,
            }
        } else {
            panic!(
                "No configuration provided for CoverImageDisplayService. Check your config file."
            )
        }
    }
}

impl SingletonService for CoverImageDisplayService {
    #[tracing::instrument(skip(self))]
    async fn run(mut self) {
        while let Ok(cmd_and_meta) = self.command_channel.recv().await {
            let cmd = cmd_and_meta.cmd;
            match cmd {
                ExtServiceCommand::Data(items) => {
                    if items.len() < 3 {
                        tracing::error!("Image data too small");
                    } else {
                        let mime: Option<&'static str> = match *items.as_slice() {
                            [0xFF, 0xD8, ..] => Some("image/jpeg"),
                            [0x89, 0x50, 0x4E, 0x47, ..] => Some("image/png"),
                            [0x47, 0x49, 0x46, 0x38, ..] => Some("image/gif"),
                            [0x52, 0x49, 0x46, 0x46, ..] => Some("image/webp"),
                            _ => {
                                tracing::error!(
                                    "Invalid image data: not a recognized image format"
                                );
                                None
                            }
                        };

                        if let Some(mime) = mime {
                            let form = reqwest::multipart::Form::new().part(
                                "file",
                                reqwest::multipart::Part::bytes(items)
                                    .mime_str(mime)
                                    .expect("Failed to set mime type")
                                    .file_name("cover.jpg"),
                            );

                            let url = self.url.to_string();
                            let client = self.client.clone();

                            let _ = tokio::task::spawn_blocking(move || {
                                match client.post(&url).multipart(form).send() {
                                    Ok(response) if response.status().is_success() => {
                                        tracing::info!("Cover image uploaded successfully");
                                        let _ = cmd_and_meta
                                            .response_channel
                                            .blocking_send(ExtServiceResponse::Ok);
                                    }
                                    Ok(response) => {
                                        tracing::error!(
                                            "Cover image upload failed: {}",
                                            response.status()
                                        );
                                        let _ = cmd_and_meta
                                            .response_channel
                                            .blocking_send(ExtServiceResponse::Error);
                                    }
                                    Err(e) => {
                                        tracing::error!("Cover image upload request failed: {}", e);
                                    }
                                }
                            })
                            .await;
                        }
                    }
                }
                ExtServiceCommand::RegisterClient => {}
                ExtServiceCommand::UnregisterClient => {}
            }
        }
    }
}
