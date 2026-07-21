use super::*;

pub(super) fn make_remote_delegate(
    this: Entity<GitStore>,
    project_id: u64,
    repository_id: RepositoryId,
    askpass_id: u64,
    cx: &mut AsyncApp,
) -> AskPassDelegate {
    AskPassDelegate::new(cx, move |prompt, tx, cx| {
        this.update(cx, |this, cx| {
            let Some((client, _)) = this.downstream_client() else {
                return;
            };
            let response = client.request(proto::AskPassRequest {
                project_id,
                repository_id: repository_id.to_proto(),
                askpass_id,
                prompt,
            });
            cx.spawn(async move |_, _| {
                let mut response = response.await?.response;
                tx.send(EncryptedPassword::try_from(response.as_ref())?)
                    .ok();
                response.zeroize();
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        });
    })
}
