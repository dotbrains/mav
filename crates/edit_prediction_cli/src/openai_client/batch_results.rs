use anyhow::Result;
use open_ai::{OPEN_AI_API_URL, batches};
use sqlez_macros::sql;

use super::batching::BatchingOpenAiClient;

impl BatchingOpenAiClient {
    pub async fn import_batches(&self, batch_ids: &[String]) -> Result<()> {
        for batch_id in batch_ids {
            log::info!("Importing OpenAI batch {}", batch_id);

            let batch_status = batches::retrieve_batch(
                self.http_client.as_ref(),
                OPEN_AI_API_URL,
                &self.api_key,
                batch_id,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to retrieve batch {}: {:?}", batch_id, e))?;

            log::info!("Batch {} status: {}", batch_id, batch_status.status);

            if batch_status.status != "completed" {
                log::warn!(
                    "Batch {} is not completed (status: {}), skipping",
                    batch_id,
                    batch_status.status
                );
                continue;
            }

            let output_file_id = batch_status.output_file_id.ok_or_else(|| {
                anyhow::anyhow!("Batch {} completed but has no output file", batch_id)
            })?;

            let results_content = batches::download_file(
                self.http_client.as_ref(),
                OPEN_AI_API_URL,
                &self.api_key,
                &output_file_id,
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to download batch results for {}: {:?}", batch_id, e)
            })?;

            let results = batches::parse_batch_output(&results_content)
                .map_err(|e| anyhow::anyhow!("Failed to parse batch output: {:?}", e))?;

            let mut updates: Vec<(String, String, String)> = Vec::new();
            let mut success_count = 0;
            let mut error_count = 0;

            for result in results {
                let request_hash = result
                    .custom_id
                    .strip_prefix("req_hash_")
                    .unwrap_or(&result.custom_id)
                    .to_string();

                if let Some(response_body) = result.response {
                    if response_body.status_code == 200 {
                        let response_json = serde_json::to_string(&response_body.body)?;
                        updates.push((request_hash, response_json, batch_id.clone()));
                        success_count += 1;
                    } else {
                        log::error!(
                            "Batch request {} failed with status {}",
                            request_hash,
                            response_body.status_code
                        );
                        let error_json = serde_json::json!({
                            "error": {
                                "type": "http_error",
                                "status_code": response_body.status_code
                            }
                        })
                        .to_string();
                        updates.push((request_hash, error_json, batch_id.clone()));
                        error_count += 1;
                    }
                } else if let Some(error) = result.error {
                    log::error!(
                        "Batch request {} failed: {}: {}",
                        request_hash,
                        error.code,
                        error.message
                    );
                    let error_json = serde_json::json!({
                        "error": {
                            "type": error.code,
                            "message": error.message
                        }
                    })
                    .to_string();
                    updates.push((request_hash, error_json, batch_id.clone()));
                    error_count += 1;
                }
            }

            let connection = self.connection.lock().unwrap();
            connection.with_savepoint("batch_import", || {
                let q = sql!(
                    INSERT OR REPLACE INTO openai_cache(request_hash, request, response, batch_id)
                    VALUES (?, (SELECT request FROM openai_cache WHERE request_hash = ?), ?, ?)
                );
                let mut exec = connection.exec_bound::<(&str, &str, &str, &str)>(q)?;
                for (request_hash, response_json, batch_id) in &updates {
                    exec((
                        request_hash.as_str(),
                        request_hash.as_str(),
                        response_json.as_str(),
                        batch_id.as_str(),
                    ))?;
                }
                Ok(())
            })?;

            log::info!(
                "Imported batch {}: {} successful, {} errors",
                batch_id,
                success_count,
                error_count
            );
        }

        Ok(())
    }

    pub(super) async fn download_finished_batches(&self) -> Result<()> {
        let batch_ids: Vec<String> = {
            let connection = self.connection.lock().unwrap();
            let q = sql!(SELECT DISTINCT batch_id FROM openai_cache WHERE batch_id IS NOT NULL AND response IS NULL);
            connection.select(q)?()?
        };

        for batch_id in &batch_ids {
            let batch_status = batches::retrieve_batch(
                self.http_client.as_ref(),
                OPEN_AI_API_URL,
                &self.api_key,
                batch_id,
            )
            .await
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;

            log::info!("Batch {} status: {}", batch_id, batch_status.status);

            if batch_status.status == "completed" {
                let output_file_id = match batch_status.output_file_id {
                    Some(id) => id,
                    None => {
                        log::warn!("Batch {} completed but has no output file", batch_id);
                        continue;
                    }
                };

                let results_content = batches::download_file(
                    self.http_client.as_ref(),
                    OPEN_AI_API_URL,
                    &self.api_key,
                    &output_file_id,
                )
                .await
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;

                let results = batches::parse_batch_output(&results_content)
                    .map_err(|e| anyhow::anyhow!("Failed to parse batch output: {:?}", e))?;

                let mut updates: Vec<(String, String)> = Vec::new();
                let mut success_count = 0;

                for result in results {
                    let request_hash = result
                        .custom_id
                        .strip_prefix("req_hash_")
                        .unwrap_or(&result.custom_id)
                        .to_string();

                    if let Some(response_body) = result.response {
                        if response_body.status_code == 200 {
                            let response_json = serde_json::to_string(&response_body.body)?;
                            updates.push((response_json, request_hash));
                            success_count += 1;
                        } else {
                            log::error!(
                                "Batch request {} failed with status {}",
                                request_hash,
                                response_body.status_code
                            );
                            let error_json = serde_json::json!({
                                "error": {
                                    "type": "http_error",
                                    "status_code": response_body.status_code
                                }
                            })
                            .to_string();
                            updates.push((error_json, request_hash));
                        }
                    } else if let Some(error) = result.error {
                        log::error!(
                            "Batch request {} failed: {}: {}",
                            request_hash,
                            error.code,
                            error.message
                        );
                        let error_json = serde_json::json!({
                            "error": {
                                "type": error.code,
                                "message": error.message
                            }
                        })
                        .to_string();
                        updates.push((error_json, request_hash));
                    }
                }

                let connection = self.connection.lock().unwrap();
                connection.with_savepoint("batch_download", || {
                    let q = sql!(UPDATE openai_cache SET response = ? WHERE request_hash = ?);
                    let mut exec = connection.exec_bound::<(&str, &str)>(q)?;
                    for (response_json, request_hash) in &updates {
                        exec((response_json.as_str(), request_hash.as_str()))?;
                    }
                    Ok(())
                })?;
                log::info!("Downloaded {} successful requests", success_count);
            }
        }

        Ok(())
    }
}
