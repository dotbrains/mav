use super::*;

impl App {
    /// Returns handles to all open windows in the application.
    /// Each handle could be downcast to a handle typed for the root view of that window.
    /// To find all windows of a given type, you could filter on
    pub fn windows(&self) -> Vec<AnyWindowHandle> {
        self.windows
            .keys()
            .flat_map(|window_id| self.window_handles.get(&window_id).copied())
            .collect()
    }
    /// Returns the window handles ordered by their appearance on screen, front to back.
    ///
    /// The first window in the returned list is the active/topmost window of the application.
    ///
    /// This method returns None if the platform doesn't implement the method yet.
    pub fn window_stack(&self) -> Option<Vec<AnyWindowHandle>> {
        self.platform.window_stack()
    }

    /// Returns a handle to the window that is currently focused at the platform level, if one exists.
    pub fn active_window(&self) -> Option<AnyWindowHandle> {
        self.platform.active_window()
    }

    /// Opens a new window with the given option and the root view returned by the given function.
    /// The function is invoked with a `Window`, which can be used to interact with window-specific
    /// functionality.
    pub fn open_window<V: 'static + Render>(
        &mut self,
        options: crate::WindowOptions,
        build_root_view: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
    ) -> anyhow::Result<WindowHandle<V>> {
        self.update(|cx| {
            let id = cx.windows.insert(None);
            let handle = WindowHandle::new(id);
            match Window::new(handle.into(), options, cx) {
                Ok(mut window) => {
                    cx.window_update_stack.push(id);
                    let root_view = build_root_view(&mut window, cx);
                    cx.window_update_stack.pop();
                    window.root.replace(root_view.into());
                    window.defer(cx, |window: &mut Window, cx| window.appearance_changed(cx));

                    // allow a window to draw at least once before returning
                    // this didn't cause any issues on non windows platforms as it seems we always won the race to on_request_frame
                    // on windows we quite frequently lose the race and return a window that has never rendered, which leads to a crash
                    // where DispatchTree::root_node_id asserts on empty nodes
                    let clear = window.draw(cx);
                    clear.clear();

                    cx.window_handles.insert(id, window.handle);
                    cx.windows.get_mut(id).unwrap().replace(Box::new(window));
                    Ok(handle)
                }
                Err(e) => {
                    cx.windows.remove(id);
                    Err(e)
                }
            }
        })
    }

    /// Instructs the platform to activate the application by bringing it to the foreground.
    pub fn activate(&self, ignoring_other_apps: bool) {
        self.platform.activate(ignoring_other_apps);
    }

    /// Hide the application at the platform level.
    pub fn hide(&self) {
        self.platform.hide();
    }

    /// Hide other applications at the platform level.
    pub fn hide_other_apps(&self) {
        self.platform.hide_other_apps();
    }

    /// Unhide other applications at the platform level.
    pub fn unhide_other_apps(&self) {
        self.platform.unhide_other_apps();
    }

    /// Returns the list of currently active displays.
    pub fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        self.platform.displays()
    }

    /// Returns the primary display that will be used for new windows.
    pub fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        self.platform.primary_display()
    }

    /// Returns whether `screen_capture_sources` may work.
    pub fn is_screen_capture_supported(&self) -> bool {
        self.platform.is_screen_capture_supported()
    }

    /// Returns a list of available screen capture sources.
    pub fn screen_capture_sources(
        &self,
    ) -> oneshot::Receiver<Result<Vec<Rc<dyn ScreenCaptureSource>>>> {
        self.platform.screen_capture_sources()
    }

    /// Returns the display with the given ID, if one exists.
    pub fn find_display(&self, id: DisplayId) -> Option<Rc<dyn PlatformDisplay>> {
        self.displays()
            .iter()
            .find(|display| display.id() == id)
            .cloned()
    }

    /// Returns the current thermal state of the system.
    pub fn thermal_state(&self) -> ThermalState {
        self.platform.thermal_state()
    }

    /// Invokes a handler when the thermal state changes
    pub fn on_thermal_state_change<F>(&self, mut callback: F) -> Subscription
    where
        F: 'static + FnMut(&mut App),
    {
        let (subscription, activate) = self.thermal_state_observers.insert(
            (),
            Box::new(move |cx| {
                callback(cx);
                true
            }),
        );
        activate();
        subscription
    }

    /// Returns the appearance of the application's windows.
    pub fn window_appearance(&self) -> WindowAppearance {
        self.platform.window_appearance()
    }

    /// Returns the window button layout configuration when supported.
    pub fn button_layout(&self) -> Option<WindowButtonLayout> {
        self.platform.button_layout()
    }

    /// Reads data from the platform clipboard.
    pub fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        self.platform.read_from_clipboard()
    }

    /// Sets the text rendering mode for the application.
    pub fn set_text_rendering_mode(&mut self, mode: TextRenderingMode) {
        self.text_rendering_mode.set(mode);
    }

    /// Returns the current text rendering mode for the application.
    pub fn text_rendering_mode(&self) -> TextRenderingMode {
        self.text_rendering_mode.get()
    }

    /// Writes data to the platform clipboard.
    pub fn write_to_clipboard(&self, item: ClipboardItem) {
        self.platform.write_to_clipboard(item)
    }

    /// Reads data from the primary selection buffer.
    /// Only available on Linux.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub fn read_from_primary(&self) -> Option<ClipboardItem> {
        self.platform.read_from_primary()
    }

    /// Writes data to the primary selection buffer.
    /// Only available on Linux.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub fn write_to_primary(&self, item: ClipboardItem) {
        self.platform.write_to_primary(item)
    }

    /// Reads data from macOS's "Find" pasteboard.
    ///
    /// Used to share the current search string between apps.
    ///
    /// https://developer.apple.com/documentation/appkit/nspasteboard/name-swift.struct/find
    #[cfg(target_os = "macos")]
    pub fn read_from_find_pasteboard(&self) -> Option<ClipboardItem> {
        self.platform.read_from_find_pasteboard()
    }

    /// Writes data to macOS's "Find" pasteboard.
    ///
    /// Used to share the current search string between apps.
    ///
    /// https://developer.apple.com/documentation/appkit/nspasteboard/name-swift.struct/find
    #[cfg(target_os = "macos")]
    pub fn write_to_find_pasteboard(&self, item: ClipboardItem) {
        self.platform.write_to_find_pasteboard(item)
    }

    /// Writes credentials to the platform keychain.
    pub fn write_credentials(
        &self,
        url: &str,
        username: &str,
        password: &[u8],
    ) -> Task<Result<()>> {
        self.platform.write_credentials(url, username, password)
    }

    /// Reads credentials from the platform keychain.
    pub fn read_credentials(&self, url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        self.platform.read_credentials(url)
    }

    /// Deletes credentials from the platform keychain.
    pub fn delete_credentials(&self, url: &str) -> Task<Result<()>> {
        self.platform.delete_credentials(url)
    }

    /// Directs the platform's default browser to open the given URL.
    pub fn open_url(&self, url: &str) {
        self.platform.open_url(url);
    }

    /// Registers the given URL scheme (e.g. `mav` for `mav://` urls) to be
    /// opened by the current app.
    ///
    /// On some platforms (e.g. macOS) you may be able to register URL schemes
    /// as part of app distribution, but this method exists to let you register
    /// schemes at runtime.
    pub fn register_url_scheme(&self, scheme: &str) -> Task<Result<()>> {
        self.platform.register_url_scheme(scheme)
    }

    /// Returns the full pathname of the current app bundle.
    ///
    /// Returns an error if the app is not being run from a bundle.
    pub fn app_path(&self) -> Result<PathBuf> {
        self.platform.app_path()
    }

    /// On Linux, returns the name of the compositor in use.
    ///
    /// Returns an empty string on other platforms.
    pub fn compositor_name(&self) -> &'static str {
        self.platform.compositor_name()
    }

    /// Returns the file URL of the executable with the specified name in the application bundle
    pub fn path_for_auxiliary_executable(&self, name: &str) -> Result<PathBuf> {
        self.platform.path_for_auxiliary_executable(name)
    }

    /// Displays a platform modal for selecting paths.
    ///
    /// When one or more paths are selected, they'll be relayed asynchronously via the returned oneshot channel.
    /// If cancelled, a `None` will be relayed instead.
    /// May return an error on Linux if the file picker couldn't be opened.
    pub fn prompt_for_paths(
        &self,
        options: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        self.platform.prompt_for_paths(options)
    }

    /// Displays a platform modal for selecting a new path where a file can be saved.
    ///
    /// The provided directory will be used to set the initial location.
    /// When a path is selected, it is relayed asynchronously via the returned oneshot channel.
    /// If cancelled, a `None` will be relayed instead.
    /// May return an error on Linux if the file picker couldn't be opened.
    pub fn prompt_for_new_path(
        &self,
        directory: &Path,
        suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        self.platform.prompt_for_new_path(directory, suggested_name)
    }

    /// Reveals the specified path at the platform level, such as in Finder on macOS.
    pub fn reveal_path(&self, path: &Path) {
        self.platform.reveal_path(path)
    }

    /// Opens the specified path with the system's default application.
    pub fn open_with_system(&self, path: &Path) {
        self.platform.open_with_system(path)
    }

    /// Returns whether the user has configured scrollbars to auto-hide at the platform level.
    pub fn should_auto_hide_scrollbars(&self) -> bool {
        self.platform.should_auto_hide_scrollbars()
    }

    /// Restarts the application.
    pub fn restart(&mut self) {
        self.restart_observers
            .clone()
            .retain(&(), |observer| observer(self));
        self.platform.restart(self.restart_path.take())
    }

    /// Sets the path to use when restarting the application.
    pub fn set_restart_path(&mut self, path: PathBuf) {
        self.restart_path = Some(path);
    }

    /// Returns the HTTP client for the application.
    pub fn http_client(&self) -> Arc<dyn HttpClient> {
        self.http_client.clone()
    }

    /// Sets the HTTP client for the application.
    pub fn set_http_client(&mut self, new_client: Arc<dyn HttpClient>) {
        self.http_client = new_client;
    }

    /// Configures when the application should automatically quit.
    /// By default, [`QuitMode::Default`] is used.
    pub fn set_quit_mode(&mut self, mode: QuitMode) {
        self.quit_mode = mode;
    }

    /// Returns the SVG renderer used by the application.
    pub fn svg_renderer(&self) -> SvgRenderer {
        self.svg_renderer.clone()
    }
}
