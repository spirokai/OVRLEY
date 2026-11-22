//! Where a [`Player`] draws: a texture of the host's wgpu device, per platform.
//!
//! - macOS: a texture of the shared Metal device, drawn on its queue.
//! - Windows: a D3D11 texture the DX12 device opens (see [`d3d11`]). The D3D12 path, a texture of
//!   the shared DX12 device drawn on its queue, is kept but not wired in (`WindowsTarget::ACTIVE`).
//! - Linux: Vulkan memory that OpenGL imports (see [`vulkan_gl`]).
//!
//! Render thread only: the thread that owns the GPU queue.

use crate::player::{decoders_from_env, Callbacks, Player};
use std::rc::Rc;

#[cfg(any(target_os = "macos", windows))]
use std::ffi::c_void;

/// What all surfaces share: the interop with the host's device and the decoders that fit it.
#[derive(Clone)]
pub struct Renderer(Rc<Shared>);

struct Shared {
    decoders: Vec<String>,
    #[cfg(any(target_os = "macos", windows))]
    device: wgpu::Device,
    #[cfg(any(target_os = "macos", windows))]
    queue: wgpu::Queue,
    /// `None` only on the D3D12 path, which is not wired in.
    #[cfg(windows)]
    d3d11: Option<d3d11::Device>,
    #[cfg(target_os = "linux")]
    bridge: vulkan_gl::Bridge,
}

impl Renderer {
    /// Fails when MDK cannot draw into textures of this device.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, String> {
        #[cfg(windows)]
        let d3d11 = match crate::player::WindowsTarget::ACTIVE {
            crate::player::WindowsTarget::D3d12 => None,
            crate::player::WindowsTarget::D3d11 => Some(
                d3d11::Device::new(device, queue)
                    .map_err(|error| format!("no D3D11 device shares with the DX12 device: {error}"))?,
            ),
        };
        #[cfg(target_os = "linux")]
        let bridge = vulkan_gl::Bridge::new(device, queue)?;
        crate::player::install_log_handler();
        let decoders = decoders_from_env();
        log::debug!("playback: MDK {}, decoders {decoders:?}", crate::player::version());
        Ok(Self(Rc::new(Shared {
            decoders,
            #[cfg(any(target_os = "macos", windows))]
            device: device.clone(),
            #[cfg(any(target_os = "macos", windows))]
            queue: queue.clone(),
            #[cfg(windows)]
            d3d11,
            #[cfg(target_os = "linux")]
            bridge,
        })))
    }

    fn create_target(&self, size: (u32, u32)) -> Result<Target, String> {
        #[cfg(target_os = "linux")]
        {
            let (gl, texture) = self.0.bridge.create_target(size)?;
            Ok(Target { texture, size, gl })
        }
        #[cfg(windows)]
        {
            let (shared, texture) = match &self.0.d3d11 {
                Some(d3d11) => {
                    let (shared, texture) = d3d11
                        .create_target(size)
                        .map_err(|error| format!("no D3D11 texture shares with the DX12 device: {error}"))?;
                    (Some(shared), texture)
                }
                None => (None, self.create_texture(size)),
            };
            Ok(Target { texture, size, shared })
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Target { texture: self.create_texture(size), size })
        }
    }

    /// A texture of the host's device, for the Metal and D3D12 targets.
    #[cfg(any(target_os = "macos", windows))]
    fn create_texture(&self, size: (u32, u32)) -> wgpu::Texture {
        self.0.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("video playback frame"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }

    /// Makes `player` draw into `target`.
    ///
    /// # Safety
    /// The target must live until the player has another one or is dropped.
    unsafe fn attach(&self, player: &Player, target: &Target) {
        #[cfg(target_os = "linux")]
        // SAFETY: the bridge's context stays current on this thread; the caller keeps the target.
        unsafe {
            player.set_gl_target(target.gl.fbo())
        };
        #[cfg(windows)]
        {
            use windows::core::Interface;
            type Dx12 = wgpu::wgc::api::Dx12;
            match &target.shared {
                // SAFETY: the caller keeps the target.
                Some(shared) => unsafe { player.set_d3d11_target(Interface::as_raw(shared) as *const c_void) },
                // MDK submits its draw on the shared queue, ahead of Slint's next frame, which samples it.
                None => unsafe {
                    let queue = self.0.queue.as_hal::<Dx12>().expect("the shared queue is DX12");
                    let resource = target.texture.as_hal::<Dx12>().expect("a DX12 texture");
                    player.set_d3d12_target(
                        Interface::as_raw(queue.as_raw()) as *const c_void,
                        Interface::as_raw(resource.raw_resource()) as *const c_void,
                    )
                },
            }
        }
        #[cfg(target_os = "macos")]
        // SAFETY: `self` keeps the device and queue; the caller keeps the target.
        unsafe {
            type Metal = wgpu::wgc::api::Metal;
            let device = self.0.device.as_hal::<Metal>().expect("the shared device is Metal");
            let queue = self.0.queue.as_hal::<Metal>().expect("the shared queue is Metal");
            let texture = target.texture.as_hal::<Metal>().expect("a Metal texture");
            player.set_metal_target(
                &**device.raw_device() as *const _ as *const c_void,
                queue.as_raw() as *const _ as *const c_void,
                texture.raw_handle() as *const _ as *const c_void,
            )
        };
        player.set_surface_size(target.size.0 as i32, target.size.1 as i32);
    }
}

struct Target {
    texture: wgpu::Texture,
    size: (u32, u32),
    /// The D3D11 texture MDK draws into, which backs `texture`; `None` only on the D3D12 path, which is not wired in.
    #[cfg(windows)]
    shared: Option<windows::Win32::Graphics::Direct3D11::ID3D11Texture2D>,
    /// MDK's framebuffer over the texture's memory.
    #[cfg(target_os = "linux")]
    gl: vulkan_gl::Target,
}

/// A texture and the player that draws into it.
///
/// Every media gets a new player, as in Gyroflow: setting other media on a player that played can
/// block the caller for good, and a stopped one often never draws the new media's first frame.
pub struct Surface {
    // In this order: the player lets go of the target, and the bridge's context goes last.
    player: Option<Player>,
    target: Target,
    renderer: Renderer,
}

impl Surface {
    pub fn new(renderer: &Renderer, size: (u32, u32)) -> Result<Self, String> {
        Ok(Self { player: None, target: renderer.create_target(size)?, renderer: renderer.clone() })
    }

    /// Opens `path` paused at `start_seconds`. The texture keeps its contents until the first frame.
    pub fn load(&mut self, path: &str, start_seconds: f64, callbacks: Callbacks) {
        self.player = None;
        let mut player = Player::new(&self.renderer.0.decoders, callbacks);
        // SAFETY: the target outlives the player (field order, and `resize` attaches before it drops).
        unsafe { self.renderer.attach(&player, &self.target) };
        player.open(path, start_seconds);
        self.player = Some(player);
    }

    /// Releases the media. The texture keeps its contents.
    pub fn unload(&mut self) {
        self.player = None;
    }

    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    /// A new texture of `size`. The current frame is drawn into it at the next [`Surface::render`].
    pub fn resize(&mut self, size: (u32, u32)) -> Result<(), String> {
        let target = self.renderer.create_target(size)?;
        if let Some(player) = &self.player {
            // SAFETY: the new target is stored below and lives as long as the player.
            unsafe { self.renderer.attach(player, &target) };
        }
        self.target = target;
        Ok(())
    }

    pub fn size(&self) -> (u32, u32) {
        self.target.size
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.target.texture
    }

    /// Draws the next or current frame: its media second, or `None` without one. Until the media
    /// is loaded nothing is drawn, so the last frame stays in the texture.
    pub fn render(&mut self) -> Option<f64> {
        let player = self.player.as_ref()?;
        let media_start = player.media_start_seconds()?;
        #[cfg(windows)]
        if let Some(d3d11) = &self.renderer.0.d3d11 {
            d3d11.before_render();
        }
        #[cfg(target_os = "linux")]
        self.renderer.0.bridge.before_render(&self.target.gl);
        let timestamp = player.render();
        #[cfg(windows)]
        if let Some(d3d11) = &self.renderer.0.d3d11 {
            d3d11.after_render();
        }
        #[cfg(target_os = "linux")]
        self.renderer.0.bridge.after_render(&mut self.target.gl);
        // MDK stamps frames on the container's clock; seeks and cuts count from the media's start.
        (timestamp >= 0.0).then_some(timestamp - media_start)
    }
}

/// The D3D11 target (Windows'): a D3D11 device on the shared device's adapter, whose
/// textures the DX12 device opens through NT handles. One fence, shared by both, orders the two
/// queues on the GPU around each `renderVideo()`; the UI thread never waits.
#[cfg(windows)]
mod d3d11 {
    use std::cell::Cell;
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, GENERIC_ALL, HANDLE, HMODULE};
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_1};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11Device5, ID3D11DeviceContext, ID3D11DeviceContext4,
        ID3D11Fence, ID3D11Multithread, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET,
        D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_FENCE_FLAG_SHARED, D3D11_RESOURCE_MISC_SHARED,
        D3D11_RESOURCE_MISC_SHARED_NTHANDLE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
        D3D11_USAGE_DEFAULT,
    };
    use windows::Win32::Graphics::Direct3D12::{
        ID3D12CommandQueue, ID3D12Device, ID3D12Fence, ID3D12Resource,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory4, IDXGIResource1, DXGI_SHARED_RESOURCE_READ,
        DXGI_SHARED_RESOURCE_WRITE,
    };

    type Dx12 = wgpu::wgc::api::Dx12;

    pub struct Device {
        device: ID3D11Device,
        context: ID3D11DeviceContext4,
        device12: ID3D12Device,
        queue: ID3D12CommandQueue,
        fence11: ID3D11Fence,
        fence12: ID3D12Fence,
        wgpu_device: wgpu::Device,
        value: Cell<u64>,
    }

    /// Opens an NT handle on the DX12 device and closes it.
    fn open_shared<T: Interface>(
        device12: &ID3D12Device,
        handle: HANDLE,
    ) -> windows::core::Result<T> {
        let mut opened: Option<T> = None;
        let result = unsafe { device12.OpenSharedHandle(handle, &mut opened) };
        unsafe { CloseHandle(handle) }?;
        result?;
        Ok(opened.expect("OpenSharedHandle succeeded"))
    }

    impl Device {
        pub fn new(wgpu_device: &wgpu::Device, wgpu_queue: &wgpu::Queue) -> windows::core::Result<Self> {
            let device12 = unsafe { wgpu_device.as_hal::<Dx12>() }
                .expect("the shared device is DX12")
                .raw_device()
                .clone();
            let queue = unsafe { wgpu_queue.as_hal::<Dx12>() }
                .expect("the shared queue is DX12")
                .as_raw()
                .clone();
            let factory: IDXGIFactory4 = unsafe { CreateDXGIFactory1() }?;
            let adapter: IDXGIAdapter =
                unsafe { factory.EnumAdapterByLuid(device12.GetAdapterLuid()) }?;

            let mut device = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            unsafe {
                D3D11CreateDevice(
                    &adapter,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                    Some(&[D3D_FEATURE_LEVEL_11_1]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )
            }?;
            let device: ID3D11Device = device.expect("D3D11CreateDevice succeeded");
            let context: ID3D11DeviceContext4 =
                context.expect("D3D11CreateDevice succeeded").cast()?;
            // MDK's decoder threads share the immediate context with renderVideo() on the UI thread.
            let _ = unsafe {
                device
                    .cast::<ID3D11Multithread>()?
                    .SetMultithreadProtected(true)
            };

            let mut fence11: Option<ID3D11Fence> = None;
            unsafe {
                device.cast::<ID3D11Device5>()?.CreateFence(
                    0,
                    D3D11_FENCE_FLAG_SHARED,
                    &mut fence11,
                )
            }?;
            let fence11 = fence11.expect("CreateFence succeeded");
            let handle =
                unsafe { fence11.CreateSharedHandle(None, GENERIC_ALL.0, PCWSTR::null()) }?;
            let fence12: ID3D12Fence = open_shared(&device12, handle)?;
            log::debug!(
                "playback: D3D11 target device on the shared adapter, fence shared with DX12"
            );
            Ok(Self {
                device,
                context,
                device12,
                queue,
                fence11,
                fence12,
                wgpu_device: wgpu_device.clone(),
                value: Cell::new(0),
            })
        }

        /// A D3D11 render target MDK draws into, and the same memory as a wgpu texture Slint samples.
        pub fn create_target(
            &self,
            size: (u32, u32),
        ) -> windows::core::Result<(ID3D11Texture2D, wgpu::Texture)> {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: size.0,
                Height: size.1,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: (D3D11_RESOURCE_MISC_SHARED.0 | D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0)
                    as u32,
            };
            let mut texture = None;
            unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut texture)) }?;
            let texture: ID3D11Texture2D = texture.expect("CreateTexture2D succeeded");
            let handle = unsafe {
                texture.cast::<IDXGIResource1>()?.CreateSharedHandle(
                    None,
                    (DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE).0,
                    PCWSTR::null(),
                )
            }?;
            let resource: ID3D12Resource = open_shared(&self.device12, handle)?;

            let extent = wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            };
            let hal = unsafe {
                wgpu::hal::dx12::Device::texture_from_raw(
                    resource,
                    wgpu::TextureFormat::Rgba8Unorm,
                    wgpu::TextureDimension::D2,
                    extent,
                    1,
                    1,
                )
            };
            // Slint only samples it. A shared texture starts in COMMON, which D3D12 promotes to a
            // shader-read state implicitly and decays back after each submission, so wgpu's tracked
            // state (sampled) never needs a barrier that would hold it against D3D11.
            let texture12 = unsafe {
                self.wgpu_device.create_texture_from_hal::<Dx12>(
                    hal,
                    &wgpu::TextureDescriptor {
                        label: Some("video playback frame (D3D11)"),
                        size: extent,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    },
                    wgpu::TextureUses::RESOURCE,
                )
            };
            Ok((texture, texture12))
        }

        /// Before MDK draws: D3D11 waits on the GPU for what DX12 already submitted (Slint's last
        /// frame, which samples the texture MDK is about to overwrite).
        pub fn before_render(&self) {
            self.value.set(self.value.get() + 1);
            unsafe {
                self.queue
                    .Signal(&self.fence12, self.value.get())
                    .expect("signal the shared fence on DX12");
                self.context
                    .Wait(&self.fence11, self.value.get())
                    .expect("wait on the shared fence on D3D11");
            }
        }

        /// After MDK drew: DX12 waits on the GPU for D3D11's draw before Slint's next frame samples it.
        pub fn after_render(&self) {
            self.value.set(self.value.get() + 1);
            unsafe {
                self.context
                    .Signal(&self.fence11, self.value.get())
                    .expect("signal the shared fence on D3D11");
                self.context.Flush();
                self.queue
                    .Wait(&self.fence12, self.value.get())
                    .expect("wait on the shared fence on DX12");
            }
        }
    }
}

/// The Linux bridge: MDK renders with OpenGL, the shared device is Vulkan (gpu.rs). The frame
/// stays on the GPU:
/// - Vulkan allocates the frame texture with exportable memory (`VK_KHR_external_memory_fd`);
///   an OpenGL context on the same GPU imports it (`GL_EXT_memory_object_fd`) as the colour
///   attachment of the framebuffer MDK draws into, and wgpu wraps the same image for Slint.
/// - Two binary semaphores shared the same way (`VK_KHR_external_semaphore_fd`,
///   `GL_EXT_semaphore_fd`) order the two queues on the GPU around each `renderVideo()`, as the
///   Windows D3D11 target's shared fence does; the UI thread never waits.
///
/// The context is an EGL context without a surface, on the EGL device whose OpenGL device UUID
/// is the Vulkan device's (`EGL_EXT_platform_device`), so it works with any vendor's driver and
/// picks the right GPU on hybrid systems. It stays current on the UI thread for the bridge's
/// life: MDK's `renderVideo()` and its teardown in `Player::drop` run there and draw in whatever
/// context is current, and nothing else on the UI thread uses OpenGL.
///
/// The image never changes queue family ownership to `VK_QUEUE_FAMILY_EXTERNAL`: wgpu cannot
/// record that barrier. Drivers that share memory between their own GL and Vulkan (Mesa, NVIDIA)
/// do not need it for an exclusive image used by one queue on each side.
#[cfg(target_os = "linux")]
mod vulkan_gl {
    use ash::vk;
    use khronos_egl as egl;
    use std::ffi::{c_void, CStr};
    use std::rc::Rc;

    type Vulkan = wgpu::wgc::api::Vulkan;
    type Egl = egl::DynamicInstance<egl::EGL1_5>;

    const GL_TEXTURE_2D: u32 = 0x0DE1;
    const GL_RENDERER: u32 = 0x1F01;
    const GL_EXTENSIONS: u32 = 0x1F03;
    const GL_RGBA8: u32 = 0x8058;
    const GL_NUM_EXTENSIONS: u32 = 0x821D;
    const GL_FRAMEBUFFER: u32 = 0x8D40;
    const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
    const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
    const GL_TEXTURE_TILING_EXT: u32 = 0x9580;
    const GL_DEDICATED_MEMORY_OBJECT_EXT: u32 = 0x9581;
    const GL_OPTIMAL_TILING_EXT: u32 = 0x9584;
    const GL_HANDLE_TYPE_OPAQUE_FD_EXT: u32 = 0x9586;
    const GL_LAYOUT_SHADER_READ_ONLY_EXT: u32 = 0x9591;
    const GL_NUM_DEVICE_UUIDS_EXT: u32 = 0x9596;
    const GL_DEVICE_UUID_EXT: u32 = 0x9597;
    /// `GL_NONE` as a layout: the contents are undefined.
    const GL_LAYOUT_UNDEFINED: u32 = 0;

    const EGL_PLATFORM_DEVICE_EXT: egl::Enum = 0x313F;

    const FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
    const HANDLE: vk::ExternalMemoryHandleTypeFlags = vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD;

    /// The OpenGL entry points the bridge calls, from `eglGetProcAddress`.
    struct Gl {
        get_error: unsafe extern "system" fn() -> u32,
        get_integerv: unsafe extern "system" fn(u32, *mut i32),
        get_string: unsafe extern "system" fn(u32) -> *const u8,
        get_stringi: unsafe extern "system" fn(u32, u32) -> *const u8,
        get_unsigned_bytei_v: unsafe extern "system" fn(u32, u32, *mut u8),
        flush: unsafe extern "system" fn(),
        finish: unsafe extern "system" fn(),
        gen_textures: unsafe extern "system" fn(i32, *mut u32),
        delete_textures: unsafe extern "system" fn(i32, *const u32),
        bind_texture: unsafe extern "system" fn(u32, u32),
        tex_parameteri: unsafe extern "system" fn(u32, u32, i32),
        gen_framebuffers: unsafe extern "system" fn(i32, *mut u32),
        delete_framebuffers: unsafe extern "system" fn(i32, *const u32),
        bind_framebuffer: unsafe extern "system" fn(u32, u32),
        framebuffer_texture_2d: unsafe extern "system" fn(u32, u32, u32, u32, i32),
        check_framebuffer_status: unsafe extern "system" fn(u32) -> u32,
        create_memory_objects: unsafe extern "system" fn(i32, *mut u32),
        delete_memory_objects: unsafe extern "system" fn(i32, *const u32),
        memory_object_parameteriv: unsafe extern "system" fn(u32, u32, *const i32),
        import_memory_fd: unsafe extern "system" fn(u32, u64, u32, i32),
        tex_storage_mem_2d: unsafe extern "system" fn(u32, i32, u32, i32, i32, u32, u64),
        gen_semaphores: unsafe extern "system" fn(i32, *mut u32),
        delete_semaphores: unsafe extern "system" fn(i32, *const u32),
        import_semaphore_fd: unsafe extern "system" fn(u32, u32, i32),
        wait_semaphore: unsafe extern "system" fn(u32, u32, *const u32, u32, *const u32, *const u32),
        signal_semaphore: unsafe extern "system" fn(u32, u32, *const u32, u32, *const u32, *const u32),
    }

    impl Gl {
        /// Loads the entry points. `eglGetProcAddress` may hand out stubs for extensions the context
        /// lacks, so the caller checks the extensions before calling any of theirs.
        #[allow(clippy::missing_transmute_annotations)] // the struct field names the type
        fn load(egl: &Egl) -> Result<Self, String> {
            macro_rules! load {
                ($name:literal) => {{
                    let function = egl
                        .get_proc_address($name)
                        .ok_or(concat!("OpenGL has no ", $name))?;
                    // SAFETY: the field's type is the entry point's signature in the GL registry.
                    unsafe { std::mem::transmute::<extern "system" fn(), _>(function) }
                }};
            }
            Ok(Self {
                get_error: load!("glGetError"),
                get_integerv: load!("glGetIntegerv"),
                get_string: load!("glGetString"),
                get_stringi: load!("glGetStringi"),
                get_unsigned_bytei_v: load!("glGetUnsignedBytei_vEXT"),
                flush: load!("glFlush"),
                finish: load!("glFinish"),
                gen_textures: load!("glGenTextures"),
                delete_textures: load!("glDeleteTextures"),
                bind_texture: load!("glBindTexture"),
                tex_parameteri: load!("glTexParameteri"),
                gen_framebuffers: load!("glGenFramebuffers"),
                delete_framebuffers: load!("glDeleteFramebuffers"),
                bind_framebuffer: load!("glBindFramebuffer"),
                framebuffer_texture_2d: load!("glFramebufferTexture2D"),
                check_framebuffer_status: load!("glCheckFramebufferStatus"),
                create_memory_objects: load!("glCreateMemoryObjectsEXT"),
                delete_memory_objects: load!("glDeleteMemoryObjectsEXT"),
                memory_object_parameteriv: load!("glMemoryObjectParameterivEXT"),
                import_memory_fd: load!("glImportMemoryFdEXT"),
                tex_storage_mem_2d: load!("glTexStorageMem2DEXT"),
                gen_semaphores: load!("glGenSemaphoresEXT"),
                delete_semaphores: load!("glDeleteSemaphoresEXT"),
                import_semaphore_fd: load!("glImportSemaphoreFdEXT"),
                wait_semaphore: load!("glWaitSemaphoreEXT"),
                signal_semaphore: load!("glSignalSemaphoreEXT"),
            })
        }

        fn string(&self, name: u32) -> String {
            let value = unsafe { (self.get_string)(name) };
            if value.is_null() {
                return String::new();
            }
            unsafe { CStr::from_ptr(value.cast()) }
                .to_string_lossy()
                .into_owned()
        }

        fn has_extension(&self, name: &str) -> bool {
            let mut count = 0;
            unsafe { (self.get_integerv)(GL_NUM_EXTENSIONS, &mut count) };
            (0..count.max(0) as u32).any(|index| {
                let value = unsafe { (self.get_stringi)(GL_EXTENSIONS, index) };
                !value.is_null()
                    && unsafe { CStr::from_ptr(value.cast()) }.to_bytes() == name.as_bytes()
            })
        }

        /// The UUIDs of the devices behind the current context.
        fn device_uuids(&self) -> Vec<[u8; vk::UUID_SIZE]> {
            let mut count = 0;
            unsafe { (self.get_integerv)(GL_NUM_DEVICE_UUIDS_EXT, &mut count) };
            (0..count.max(0) as u32)
                .map(|index| {
                    let mut uuid = [0; vk::UUID_SIZE];
                    unsafe {
                        (self.get_unsigned_bytei_v)(GL_DEVICE_UUID_EXT, index, uuid.as_mut_ptr())
                    };
                    uuid
                })
                .collect()
        }

        fn check(&self, what: &str) -> Result<(), String> {
            match unsafe { (self.get_error)() } {
                0 => Ok(()),
                error => Err(format!("OpenGL error {error:#x} {what}")),
            }
        }
    }

    /// An EGL context on the Vulkan device's GPU, current on this thread.
    struct Context {
        egl: Egl,
        display: egl::Display,
        context: egl::Context,
        gl: Gl,
    }

    impl Context {
        /// The first EGL device (then the default display) whose OpenGL context runs on the device
        /// with `uuid` and can import Vulkan memory and semaphores.
        fn on_device(uuid: [u8; vk::UUID_SIZE]) -> Result<Self, String> {
            // SAFETY: libEGL.so.1 is the EGL ABI (glvnd's dispatcher or a vendor's library).
            let egl = unsafe { Egl::load_required() }
                .map_err(|error| format!("could not load EGL 1.5: {error}"))?;
            let client = egl
                .query_string(None, egl::EXTENSIONS)
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut displays = Vec::new();
            if client.contains("EGL_EXT_device_enumeration")
                && client.contains("EGL_EXT_platform_device")
            {
                for device in query_devices(&egl) {
                    // SAFETY: `device` is an EGLDeviceEXT from eglQueryDevicesEXT.
                    match unsafe {
                        egl.get_platform_display(EGL_PLATFORM_DEVICE_EXT, device, &[egl::ATTRIB_NONE])
                    } {
                        Ok(display) => displays.push(display),
                        Err(error) => log::debug!("playback: EGL device display: {error}"),
                    }
                }
            }
            // SAFETY: EGL_DEFAULT_DISPLAY needs no native display.
            if let Some(display) = unsafe { egl.get_display(egl::DEFAULT_DISPLAY) } {
                displays.push(display);
            }

            let mut rejected = Vec::new();
            for display in displays {
                match Self::try_display(&egl, display, uuid) {
                    Ok((context, gl)) => {
                        return Ok(Self {
                            egl,
                            display,
                            context,
                            gl,
                        })
                    }
                    Err(reason) => rejected.push(reason),
                }
            }
            Err(format!(
                "no EGL display shares memory with the Vulkan device: {}",
                rejected.join("; ")
            ))
        }

        fn try_display(
            egl: &Egl,
            display: egl::Display,
            uuid: [u8; vk::UUID_SIZE],
        ) -> Result<(egl::Context, Gl), String> {
            egl.initialize(display)
                .map_err(|error| format!("eglInitialize: {error}"))?;
            let reject = |reason: String| {
                let _ = egl.terminate(display);
                reason
            };
            let extensions = egl
                .query_string(Some(display), egl::EXTENSIONS)
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default();
            for needed in ["EGL_KHR_surfaceless_context", "EGL_KHR_no_config_context"] {
                if !extensions.contains(needed) {
                    return Err(reject(format!("a display without {needed}")));
                }
            }
            egl.bind_api(egl::OPENGL_API)
                .map_err(|error| reject(format!("eglBindAPI(OpenGL): {error}")))?;
            // SAFETY: EGL_NO_CONFIG_KHR, which EGL_KHR_no_config_context allows.
            let no_config = unsafe { egl::Config::from_ptr(std::ptr::null_mut()) };
            let context = egl
                .create_context(display, no_config, None, &[egl::NONE])
                .map_err(|error| reject(format!("eglCreateContext: {error}")))?;
            let reject_context = |reason: String| {
                let _ = egl.make_current(display, None, None, None);
                let _ = egl.destroy_context(display, context);
                reject(reason)
            };
            egl.make_current(display, None, None, Some(context))
                .map_err(|error| reject_context(format!("eglMakeCurrent: {error}")))?;
            let gl = Gl::load(egl).map_err(&reject_context)?;
            let renderer = gl.string(GL_RENDERER);
            for needed in ["GL_EXT_memory_object_fd", "GL_EXT_semaphore_fd"] {
                if !gl.has_extension(needed) {
                    return Err(reject_context(format!("{renderer} lacks {needed}")));
                }
            }
            if !gl.device_uuids().contains(&uuid) {
                return Err(reject_context(format!("{renderer} is another GPU")));
            }
            log::debug!("playback: OpenGL bridge context on {renderer}");
            Ok((context, gl))
        }

        fn make_current(&self) {
            self.egl
                .make_current(self.display, None, None, Some(self.context))
                .expect("the bridge's EGL context stays valid");
        }
    }

    impl Drop for Context {
        fn drop(&mut self) {
            let _ = self.egl.make_current(self.display, None, None, None);
            let _ = self.egl.destroy_context(self.display, self.context);
            let _ = self.egl.terminate(self.display);
        }
    }

    /// The EGL devices (`eglQueryDevicesEXT`).
    fn query_devices(egl: &Egl) -> Vec<*mut c_void> {
        type QueryDevices = unsafe extern "system" fn(i32, *mut *mut c_void, *mut i32) -> u32;
        let Some(function) = egl.get_proc_address("eglQueryDevicesEXT") else {
            return Vec::new();
        };
        // SAFETY: eglQueryDevicesEXT's signature (EGL_EXT_device_enumeration).
        let query: QueryDevices =
            unsafe { std::mem::transmute::<extern "system" fn(), QueryDevices>(function) };
        let mut count = 0;
        if unsafe { query(0, std::ptr::null_mut(), &mut count) } == 0 || count <= 0 {
            return Vec::new();
        }
        let mut devices = vec![std::ptr::null_mut(); count as usize];
        if unsafe { query(count, devices.as_mut_ptr(), &mut count) } == 0 {
            return Vec::new();
        }
        devices.truncate(count.max(0) as usize);
        devices
    }

    /// A binary semaphore created on Vulkan and imported into OpenGL.
    struct SharedSemaphore {
        vulkan: vk::Semaphore,
        gl: u32,
    }

    pub struct Bridge {
        device: wgpu::Device,
        queue: wgpu::Queue,
        raw_device: ash::Device,
        memory_types: vk::PhysicalDeviceMemoryProperties,
        memory_fd: ash::khr::external_memory_fd::Device,
        /// Vulkan signals it after its last use of the frame (Skia's last draw sampling it); OpenGL
        /// waits on it before MDK draws.
        to_gl: SharedSemaphore,
        /// OpenGL signals it after MDK drew; Vulkan waits on it before Skia's next draw samples.
        to_vulkan: SharedSemaphore,
        gl: Rc<Context>,
    }

    /// The frame texture's OpenGL side: MDK's framebuffer, over the memory Vulkan exported.
    pub struct Target {
        context: Rc<Context>,
        texture: u32,
        memory: u32,
        fbo: u32,
        /// OpenGL wrote the image at least once: its layout is shader-read-only (what wgpu and Skia
        /// assume) instead of undefined.
        written: bool,
    }

    impl Target {
        pub fn fbo(&self) -> u32 {
            self.fbo
        }
    }

    impl Drop for Target {
        /// OpenGL defers the deletes until its queued work is done; Vulkan's side is wgpu's texture.
        fn drop(&mut self) {
            let gl = &self.context.gl;
            self.context.make_current();
            unsafe {
                (gl.delete_framebuffers)(1, &self.fbo);
                (gl.delete_textures)(1, &self.texture);
                (gl.delete_memory_objects)(1, &self.memory);
            }
        }
    }

    impl Bridge {
        pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, String> {
            let hal_device = unsafe { device.as_hal::<Vulkan>() }
                .expect("the shared device is Vulkan on Linux");
            for needed in [
                ash::khr::external_memory_fd::NAME,
                ash::khr::external_semaphore_fd::NAME,
            ] {
                if !hal_device.enabled_device_extensions().contains(&needed) {
                    return Err(format!(
                        "the Vulkan device has no {}",
                        needed.to_string_lossy()
                    ));
                }
            }
            let instance = hal_device.shared_instance().raw_instance();
            let physical = hal_device.raw_physical_device();
            let raw_device = hal_device.raw_device().clone();
            let mut id = vk::PhysicalDeviceIDProperties::default();
            let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
            unsafe { instance.get_physical_device_properties2(physical, &mut properties) };
            let uuid = id.device_uuid;
            let memory_types = unsafe { instance.get_physical_device_memory_properties(physical) };
            let memory_fd = ash::khr::external_memory_fd::Device::new(instance, &raw_device);
            let semaphore_fd = ash::khr::external_semaphore_fd::Device::new(instance, &raw_device);
            drop(hal_device);

            let gl = Rc::new(Context::on_device(uuid)?);
            let share = |label: &str| -> Result<SharedSemaphore, String> {
                let mut export = vk::ExportSemaphoreCreateInfo::default()
                    .handle_types(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD);
                let vulkan = unsafe {
                    raw_device.create_semaphore(
                        &vk::SemaphoreCreateInfo::default().push_next(&mut export),
                        None,
                    )
                }
                .map_err(|error| format!("vkCreateSemaphore ({label}): {error}"))?;
                let fd = unsafe {
                    semaphore_fd.get_semaphore_fd(
                        &vk::SemaphoreGetFdInfoKHR::default()
                            .semaphore(vulkan)
                            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD),
                    )
                }
                .map_err(|error| {
                    unsafe { raw_device.destroy_semaphore(vulkan, None) };
                    format!("vkGetSemaphoreFdKHR ({label}): {error}")
                })?;
                let mut imported = 0;
                unsafe {
                    (gl.gl.gen_semaphores)(1, &mut imported);
                    // OpenGL owns the descriptor from here.
                    (gl.gl.import_semaphore_fd)(imported, GL_HANDLE_TYPE_OPAQUE_FD_EXT, fd);
                }
                gl.gl.check(&format!("importing the {label} semaphore"))?;
                Ok(SharedSemaphore {
                    vulkan,
                    gl: imported,
                })
            };
            let to_gl = share("Vulkan → OpenGL")?;
            let to_vulkan = share("OpenGL → Vulkan")?;
            log::debug!("playback: Vulkan–OpenGL bridge with shared memory and semaphores");
            Ok(Self {
                device: device.clone(),
                queue: queue.clone(),
                raw_device,
                memory_types,
                memory_fd,
                to_gl,
                to_vulkan,
                gl,
            })
        }

        /// A Vulkan image with exportable memory as a wgpu texture Slint samples, and its memory
        /// imported into OpenGL as MDK's framebuffer.
        pub fn create_target(&self, size: (u32, u32)) -> Result<(Target, wgpu::Texture), String> {
            let device = &self.raw_device;
            let mut external = vk::ExternalMemoryImageCreateInfo::default().handle_types(HANDLE);
            let info = vk::ImageCreateInfo::default()
                .push_next(&mut external)
                .image_type(vk::ImageType::TYPE_2D)
                .format(FORMAT)
                .extent(vk::Extent3D {
                    width: size.0,
                    height: size.1,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let image = unsafe { device.create_image(&info, None) }
                .map_err(|error| format!("vkCreateImage: {error}"))?;
            let requirements = unsafe { device.get_image_memory_requirements(image) };
            let memory_type = self
                .memory_types
                .memory_types_as_slice()
                .iter()
                .enumerate()
                .position(|(index, kind)| {
                    requirements.memory_type_bits & (1 << index) != 0
                        && kind
                            .property_flags
                            .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
                });
            let Some(memory_type) = memory_type else {
                unsafe { device.destroy_image(image, None) };
                return Err("no device-local memory type for the frame".into());
            };
            // Dedicated: OpenGL imports it as a dedicated memory object, which must match.
            let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
            let mut export = vk::ExportMemoryAllocateInfo::default().handle_types(HANDLE);
            let allocate = vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type as u32)
                .push_next(&mut dedicated)
                .push_next(&mut export);
            let memory = match unsafe { device.allocate_memory(&allocate, None) } {
                Ok(memory) => memory,
                Err(error) => {
                    unsafe { device.destroy_image(image, None) };
                    return Err(format!("vkAllocateMemory: {error}"));
                }
            };
            let fd = unsafe { device.bind_image_memory(image, memory, 0) }.and_then(|()| unsafe {
                self.memory_fd.get_memory_fd(
                    &vk::MemoryGetFdInfoKHR::default()
                        .memory(memory)
                        .handle_type(HANDLE),
                )
            });
            let fd = match fd {
                Ok(fd) => fd,
                Err(error) => {
                    unsafe {
                        device.destroy_image(image, None);
                        device.free_memory(memory, None);
                    }
                    return Err(format!("exporting the frame's memory: {error}"));
                }
            };

            // wgpu owns the image and its memory from here, and frees them once the GPU is done.
            let extent = wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            };
            let hal_texture = {
                let hal_device =
                    unsafe { self.device.as_hal::<Vulkan>() }.expect("the shared device is Vulkan");
                unsafe {
                    hal_device.texture_from_raw(
                        image,
                        &wgpu::hal::TextureDescriptor {
                            label: Some("video playback frame (OpenGL)"),
                            size: extent,
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            usage: wgpu::TextureUses::RESOURCE | wgpu::TextureUses::COLOR_TARGET,
                            memory_flags: wgpu::hal::MemoryFlags::empty(),
                            view_formats: Vec::new(),
                        },
                        None,
                        wgpu::hal::vulkan::TextureMemory::Dedicated(memory),
                    )
                }
            };
            // Slint only samples it, in the layout OpenGL leaves it in after each draw.
            let texture = unsafe {
                self.device.create_texture_from_hal::<Vulkan>(
                    hal_texture,
                    &wgpu::TextureDescriptor {
                        label: Some("video playback frame (OpenGL)"),
                        size: extent,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    },
                    wgpu::TextureUses::RESOURCE,
                )
            };

            let context = Rc::clone(&self.gl);
            context.make_current();
            let gl = &context.gl;
            let mut target = Target {
                context: Rc::clone(&context),
                texture: 0,
                memory: 0,
                fbo: 0,
                written: false,
            };
            unsafe {
                (gl.create_memory_objects)(1, &mut target.memory);
                (gl.memory_object_parameteriv)(target.memory, GL_DEDICATED_MEMORY_OBJECT_EXT, &1);
                // OpenGL owns the descriptor from here.
                (gl.import_memory_fd)(
                    target.memory,
                    requirements.size,
                    GL_HANDLE_TYPE_OPAQUE_FD_EXT,
                    fd,
                );
                (gl.gen_textures)(1, &mut target.texture);
                (gl.bind_texture)(GL_TEXTURE_2D, target.texture);
                (gl.tex_parameteri)(
                    GL_TEXTURE_2D,
                    GL_TEXTURE_TILING_EXT,
                    GL_OPTIMAL_TILING_EXT as i32,
                );
                (gl.tex_storage_mem_2d)(
                    GL_TEXTURE_2D,
                    1,
                    GL_RGBA8,
                    size.0 as i32,
                    size.1 as i32,
                    target.memory,
                    0,
                );
                (gl.bind_texture)(GL_TEXTURE_2D, 0);
                (gl.gen_framebuffers)(1, &mut target.fbo);
                (gl.bind_framebuffer)(GL_FRAMEBUFFER, target.fbo);
                (gl.framebuffer_texture_2d)(
                    GL_FRAMEBUFFER,
                    GL_COLOR_ATTACHMENT0,
                    GL_TEXTURE_2D,
                    target.texture,
                    0,
                );
            }
            let status = unsafe { (gl.check_framebuffer_status)(GL_FRAMEBUFFER) };
            unsafe { (gl.bind_framebuffer)(GL_FRAMEBUFFER, 0) };
            gl.check("importing the frame's memory")?;
            if status != GL_FRAMEBUFFER_COMPLETE {
                return Err(format!(
                    "the frame's framebuffer is incomplete ({status:#x})"
                ));
            }
            Ok((target, texture))
        }

        /// Before MDK draws: OpenGL waits on the GPU for what Vulkan already submitted (Slint's last
        /// frame, which samples the texture MDK is about to overwrite). Skia submits on the same
        /// VkQueue as wgpu, so the signal, in a submission of its own, follows Skia's work too.
        pub fn before_render(&self, target: &Target) {
            unsafe { self.queue.as_hal::<Vulkan>() }
                .expect("the shared queue is Vulkan")
                .add_signal_semaphore(self.to_gl.vulkan, None);
            self.queue.submit([]);
            self.gl.make_current();
            let layout = if target.written {
                GL_LAYOUT_SHADER_READ_ONLY_EXT
            } else {
                GL_LAYOUT_UNDEFINED
            };
            unsafe {
                (self.gl.gl.wait_semaphore)(
                    self.to_gl.gl,
                    0,
                    std::ptr::null(),
                    1,
                    &target.texture,
                    &layout,
                )
            };
        }

        /// After MDK drew: Vulkan waits on the GPU for OpenGL's draw before Slint's next frame samples
        /// it. The wait goes in a submission of its own: a semaphore wait also holds back everything
        /// submitted after it on the queue, Skia's draws included.
        pub fn after_render(&self, target: &mut Target) {
            let layout = GL_LAYOUT_SHADER_READ_ONLY_EXT;
            unsafe {
                (self.gl.gl.signal_semaphore)(
                    self.to_vulkan.gl,
                    0,
                    std::ptr::null(),
                    1,
                    &target.texture,
                    &layout,
                );
                // Submits the signal, which Vulkan's wait below must follow.
                (self.gl.gl.flush)();
            }
            target.written = true;
            unsafe { self.queue.as_hal::<Vulkan>() }
                .expect("the shared queue is Vulkan")
                .add_wait_semaphore(
                    self.to_vulkan.vulkan,
                    None,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                );
            self.queue.submit([]);
        }
    }

    impl Drop for Bridge {
        /// Neither queue may still use the semaphores.
        fn drop(&mut self) {
            self.gl.make_current();
            unsafe {
                (self.gl.gl.finish)();
                (self.gl.gl.delete_semaphores)(1, &self.to_gl.gl);
                (self.gl.gl.delete_semaphores)(1, &self.to_vulkan.gl);
            }
            if let Err(error) = self.device.poll(wgpu::PollType::wait_indefinitely()) {
                log::error!("playback: waiting for the GPU before the bridge's teardown: {error}");
            }
            unsafe {
                self.raw_device.destroy_semaphore(self.to_gl.vulkan, None);
                self.raw_device
                    .destroy_semaphore(self.to_vulkan.vulkan, None);
            }
        }
    }
}
