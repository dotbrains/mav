use super::*;

impl LspCommand for GetDefinitions {
    type Response = Vec<LocationLink>;
    type LspRequest = lsp::request::GotoDefinition;
    type ProtoRequest = proto::GetDefinition;

    fn display_name(&self) -> &str {
        "Get definition"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        capabilities
            .server_capabilities
            .definition_provider
            .is_some_and(|capability| match capability {
                OneOf::Left(supported) => supported,
                OneOf::Right(_options) => true,
            })
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::GotoDefinitionParams> {
        Ok(lsp::GotoDefinitionParams {
            text_document_position_params: make_lsp_text_document_position(path, self.position)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<lsp::GotoDefinitionResponse>,
        lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<Vec<LocationLink>> {
        location_links_from_lsp(
            message,
            lsp_store,
            buffer,
            server_id,
            self.workspace_only,
            cx,
        )
        .await
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::GetDefinition {
        proto::GetDefinition {
            project_id,
            buffer_id: buffer.remote_id().into(),
            position: Some(language::proto::serialize_anchor(
                &buffer.anchor_before(self.position),
            )),
            version: serialize_version(&buffer.version()),
            workspace_only: self.workspace_only,
        }
    }

    async fn from_proto(
        message: proto::GetDefinition,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        let position = message
            .position
            .and_then(deserialize_anchor)
            .context("invalid position")?;
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;
        Ok(Self {
            position: buffer.read_with(&cx, |buffer, _| position.to_point_utf16(buffer)),
            workspace_only: message.workspace_only,
        })
    }

    fn response_to_proto(
        response: Vec<LocationLink>,
        lsp_store: &mut LspStore,
        peer_id: PeerId,
        _: &clock::Global,
        cx: &mut App,
    ) -> proto::GetDefinitionResponse {
        let links = location_links_to_proto(response, lsp_store, peer_id, cx);
        proto::GetDefinitionResponse { links }
    }

    async fn response_from_proto(
        self,
        message: proto::GetDefinitionResponse,
        lsp_store: Entity<LspStore>,
        _: Entity<Buffer>,
        cx: AsyncApp,
    ) -> Result<Vec<LocationLink>> {
        location_links_from_proto(message.links, lsp_store, cx).await
    }

    fn buffer_id_from_proto(message: &proto::GetDefinition) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}

impl LspCommand for GetDeclarations {
    type Response = Vec<LocationLink>;
    type LspRequest = lsp::request::GotoDeclaration;
    type ProtoRequest = proto::GetDeclaration;

    fn display_name(&self) -> &str {
        "Get declaration"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        capabilities
            .server_capabilities
            .declaration_provider
            .is_some_and(|capability| match capability {
                lsp::DeclarationCapability::Simple(supported) => supported,
                lsp::DeclarationCapability::RegistrationOptions(..) => true,
                lsp::DeclarationCapability::Options(..) => true,
            })
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::GotoDeclarationParams> {
        Ok(lsp::GotoDeclarationParams {
            text_document_position_params: make_lsp_text_document_position(path, self.position)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<lsp::GotoDeclarationResponse>,
        lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<Vec<LocationLink>> {
        location_links_from_lsp(message, lsp_store, buffer, server_id, false, cx).await
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::GetDeclaration {
        proto::GetDeclaration {
            project_id,
            buffer_id: buffer.remote_id().into(),
            position: Some(language::proto::serialize_anchor(
                &buffer.anchor_before(self.position),
            )),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        message: proto::GetDeclaration,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        let position = message
            .position
            .and_then(deserialize_anchor)
            .context("invalid position")?;
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;
        Ok(Self {
            position: buffer.read_with(&cx, |buffer, _| position.to_point_utf16(buffer)),
        })
    }

    fn response_to_proto(
        response: Vec<LocationLink>,
        lsp_store: &mut LspStore,
        peer_id: PeerId,
        _: &clock::Global,
        cx: &mut App,
    ) -> proto::GetDeclarationResponse {
        let links = location_links_to_proto(response, lsp_store, peer_id, cx);
        proto::GetDeclarationResponse { links }
    }

    async fn response_from_proto(
        self,
        message: proto::GetDeclarationResponse,
        lsp_store: Entity<LspStore>,
        _: Entity<Buffer>,
        cx: AsyncApp,
    ) -> Result<Vec<LocationLink>> {
        location_links_from_proto(message.links, lsp_store, cx).await
    }

    fn buffer_id_from_proto(message: &proto::GetDeclaration) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}

impl LspCommand for GetImplementations {
    type Response = Vec<LocationLink>;
    type LspRequest = lsp::request::GotoImplementation;
    type ProtoRequest = proto::GetImplementation;

    fn display_name(&self) -> &str {
        "Get implementation"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        capabilities
            .server_capabilities
            .implementation_provider
            .is_some_and(|capability| match capability {
                lsp::ImplementationProviderCapability::Simple(enabled) => enabled,
                lsp::ImplementationProviderCapability::Options(_options) => true,
            })
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::GotoImplementationParams> {
        Ok(lsp::GotoImplementationParams {
            text_document_position_params: make_lsp_text_document_position(path, self.position)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<lsp::GotoImplementationResponse>,
        lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<Vec<LocationLink>> {
        location_links_from_lsp(message, lsp_store, buffer, server_id, false, cx).await
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::GetImplementation {
        proto::GetImplementation {
            project_id,
            buffer_id: buffer.remote_id().into(),
            position: Some(language::proto::serialize_anchor(
                &buffer.anchor_before(self.position),
            )),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        message: proto::GetImplementation,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        let position = message
            .position
            .and_then(deserialize_anchor)
            .context("invalid position")?;
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;
        Ok(Self {
            position: buffer.read_with(&cx, |buffer, _| position.to_point_utf16(buffer)),
        })
    }

    fn response_to_proto(
        response: Vec<LocationLink>,
        lsp_store: &mut LspStore,
        peer_id: PeerId,
        _: &clock::Global,
        cx: &mut App,
    ) -> proto::GetImplementationResponse {
        let links = location_links_to_proto(response, lsp_store, peer_id, cx);
        proto::GetImplementationResponse { links }
    }

    async fn response_from_proto(
        self,
        message: proto::GetImplementationResponse,
        project: Entity<LspStore>,
        _: Entity<Buffer>,
        cx: AsyncApp,
    ) -> Result<Vec<LocationLink>> {
        location_links_from_proto(message.links, project, cx).await
    }

    fn buffer_id_from_proto(message: &proto::GetImplementation) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}

impl LspCommand for GetTypeDefinitions {
    type Response = Vec<LocationLink>;
    type LspRequest = lsp::request::GotoTypeDefinition;
    type ProtoRequest = proto::GetTypeDefinition;

    fn display_name(&self) -> &str {
        "Get type definition"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        !matches!(
            &capabilities.server_capabilities.type_definition_provider,
            None | Some(lsp::TypeDefinitionProviderCapability::Simple(false))
        )
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::GotoTypeDefinitionParams> {
        Ok(lsp::GotoTypeDefinitionParams {
            text_document_position_params: make_lsp_text_document_position(path, self.position)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<lsp::GotoTypeDefinitionResponse>,
        project: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<Vec<LocationLink>> {
        location_links_from_lsp(message, project, buffer, server_id, self.workspace_only, cx).await
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::GetTypeDefinition {
        proto::GetTypeDefinition {
            project_id,
            buffer_id: buffer.remote_id().into(),
            position: Some(language::proto::serialize_anchor(
                &buffer.anchor_before(self.position),
            )),
            version: serialize_version(&buffer.version()),
            workspace_only: self.workspace_only,
        }
    }

    async fn from_proto(
        message: proto::GetTypeDefinition,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        let position = message
            .position
            .and_then(deserialize_anchor)
            .context("invalid position")?;
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;
        Ok(Self {
            position: buffer.read_with(&cx, |buffer, _| position.to_point_utf16(buffer)),
            workspace_only: message.workspace_only,
        })
    }

    fn response_to_proto(
        response: Vec<LocationLink>,
        lsp_store: &mut LspStore,
        peer_id: PeerId,
        _: &clock::Global,
        cx: &mut App,
    ) -> proto::GetTypeDefinitionResponse {
        let links = location_links_to_proto(response, lsp_store, peer_id, cx);
        proto::GetTypeDefinitionResponse { links }
    }

    async fn response_from_proto(
        self,
        message: proto::GetTypeDefinitionResponse,
        project: Entity<LspStore>,
        _: Entity<Buffer>,
        cx: AsyncApp,
    ) -> Result<Vec<LocationLink>> {
        location_links_from_proto(message.links, project, cx).await
    }

    fn buffer_id_from_proto(message: &proto::GetTypeDefinition) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
