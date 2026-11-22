//! Where a [`Player`] draws: a texture of the host's wgpu device, per platform.
//!
//! - macOS: a texture of the host's Metal device, drawn on its queue.
//! - Windows: a D3D11 texture the DX12 device opens (`d3d11`). The D3D12 path, a texture of the
//!   host's DX12 device drawn on its queue, is kept but not selected ([`WindowsTarget::ACTIVE`]).
//! - Linux: Vulkan memory that OpenGL imports (`opengl`).
//!
//! Render thread only: the thread that owns the GPU queue.

use crate::player::{self, Callbacks, Player};
use crate::{Error, Kind};
use std::fmt;
use std::rc::Rc;

/// Opens a device on `adapter` with what playback needs, where the adapter has it; otherwise as
/// `Adapter::request_device` would, and [`Renderer::new`] reports what is missing.
///
/// On Linux the Vulkan–OpenGL bridge shares semaphores through `VK_KHR_external_semaphore_fd`,
/// which wgpu does not enable itself (it enables `VK_KHR_external_memory_fd` when the driver has
/// it).
///
/// # Errors
/// When the adapter cannot open the device.
pub fn request_device(
    adapter: &wgpu::Adapter,
    descriptor: &wgpu::DeviceDescriptor<'_>,
) -> Result<(wgpu::Device, wgpu::Queue), Error> {
    #[cfg(target_os = "linux")]
    {
        type Vulkan = wgpu::wgc::api::Vulkan;
        let semaphore_fd = ash::khr::external_semaphore_fd::NAME;
        // SAFETY: the adapter is only read.
        let opened = unsafe { adapter.as_hal::<Vulkan>() }
            .filter(|hal| {
                hal.physical_device_capabilities()
                    .supports_extension(semaphore_fd)
            })
            .map(|hal| {
                let callback: Box<wgpu::hal::vulkan::CreateDeviceCallback> =
                    Box::new(move |args| args.extensions.push(semaphore_fd));
                // SAFETY: the callback only adds an extension the adapter supports.
                unsafe {
                    hal.open_with_callback(
                        descriptor.required_features,
                        &descriptor.required_limits,
                        &descriptor.memory_hints,
                        Some(callback),
                    )
                }
            });
        if let Some(opened) = opened {
            let opened = opened.map_err(Kind::from)?;
            // SAFETY: opened from this adapter with the descriptor's features and limits.
            return Ok(
                unsafe { adapter.create_device_from_hal(opened, descriptor) }
                    .map_err(Kind::from)?,
            );
        }
    }
    Ok(pollster::block_on(adapter.request_device(descriptor)).map_err(Kind::from)?)
}

/// The Windows render targets. The decoders follow the target, so that frames stay on one D3D API.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowsTarget {
    D3d11,
    D3d12,
}

#[cfg(windows)]
impl WindowsTarget {
    /// The target playback uses.
    // The D3D12 path is intentionally left in, compiled but unselected: it may be opted in in the
    // future by setting this to `Self::D3d12`.
    pub(crate) const ACTIVE: Self = Self::D3d11;

    /// MDK's decoders for the target: `D3D11`; or `D3D12` (FFmpeg's d3d12va) and `MFT:d3d=12`.
    pub(crate) const fn decoders(self) -> &'static str {
        match self {
            Self::D3d11 => "D3D11,FFmpeg",
            Self::D3d12 => "D3D12,MFT:d3d=12,FFmpeg",
        }
    }
}

/// What all surfaces share: the interop with the host's device and the decoders that fit it.
/// Clones share it; make one per device.
#[derive(Clone)]
pub struct Renderer(Rc<Shared>);

struct Shared {
    decoders: String,
    max_side: u32,
    #[cfg(any(target_os = "macos", windows))]
    device: wgpu::Device,
    #[cfg(any(target_os = "macos", windows))]
    queue: wgpu::Queue,
    /// `None` only on the D3D12 path.
    #[cfg(windows)]
    d3d11: Option<crate::d3d11::Device>,
    #[cfg(target_os = "linux")]
    bridge: crate::opengl::Bridge,
}

impl fmt::Debug for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Renderer")
            .field("decoders", &self.0.decoders)
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// # Errors
    /// When MDK cannot draw into textures of this device: another backend than the platform's, or
    /// missing interop.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, Error> {
        #[cfg(target_os = "macos")]
        // SAFETY: the device is only checked for its backend.
        if unsafe { device.as_hal::<wgpu::wgc::api::Metal>() }.is_none() {
            return Err(Kind::Backend("Metal").into());
        }
        #[cfg(windows)]
        let d3d11 = match WindowsTarget::ACTIVE {
            WindowsTarget::D3d11 => Some(crate::d3d11::Device::new(device, queue)?),
            // SAFETY: the device is only checked for its backend.
            WindowsTarget::D3d12 if unsafe { device.as_hal::<wgpu::wgc::api::Dx12>() }.is_none() => {
                return Err(Kind::Backend("DX12").into());
            }
            WindowsTarget::D3d12 => None,
        };
        #[cfg(target_os = "linux")]
        let bridge = crate::opengl::Bridge::new(device, queue)?;
        player::install_log_handler();
        let decoders = player::decoders();
        log::debug!("playback: MDK {}, decoders {decoders}", player::version());
        Ok(Self(Rc::new(Shared {
            decoders,
            max_side: device.limits().max_texture_dimension_2d,
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

    /// The longest side a [`Surface`] may have on this device.
    #[must_use]
    pub fn max_side(&self) -> u32 {
        self.0.max_side
    }

    fn create_target(&self, size: (u32, u32)) -> Result<Target, Error> {
        let max = self.0.max_side;
        if !(1..=max).contains(&size.0) || !(1..=max).contains(&size.1) {
            return Err(Kind::Size {
                width: size.0,
                height: size.1,
                max,
            }
            .into());
        }
        #[cfg(target_os = "linux")]
        {
            let (gl, texture) = self.0.bridge.create_target(size)?;
            Ok(Target { texture, size, gl })
        }
        #[cfg(windows)]
        {
            let (shared, texture) = match &self.0.d3d11 {
                Some(d3d11) => {
                    let (shared, texture) = d3d11.create_target(size)?;
                    (Some(shared), texture)
                }
                None => (None, self.create_texture(size)),
            };
            Ok(Target {
                texture,
                size,
                shared,
            })
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Target {
                texture: self.create_texture(size),
                size,
            })
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
        // SAFETY: the bridge's context is current whenever the player draws or is dropped
        // (`Surface::render`, `Surface::unload`); the caller keeps the target.
        unsafe {
            player.set_gl_target(target.gl.fbo());
        }
        #[cfg(windows)]
        {
            use windows::core::Interface;
            type Dx12 = wgpu::wgc::api::Dx12;
            match &target.shared {
                // SAFETY: the caller keeps the target.
                Some(shared) => unsafe { player.set_d3d11_target(Interface::as_raw(shared).cast_const()) },
                // MDK submits its draw on the host's queue, ahead of the host's next frame, which
                // samples it.
                // SAFETY: `self` keeps the queue, and the caller the target; the backend was checked
                // in `new`.
                None => unsafe {
                    let queue = self.0.queue.as_hal::<Dx12>().expect("checked in Renderer::new");
                    let resource = target.texture.as_hal::<Dx12>().expect("made on the device");
                    player.set_d3d12_target(
                        Interface::as_raw(queue.as_raw()).cast_const(),
                        Interface::as_raw(resource.raw_resource()).cast_const(),
                    );
                },
            }
        }
        #[cfg(target_os = "macos")]
        // SAFETY: `self` keeps the device and queue, and the caller the target; the backend was
        // checked in `new`.
        unsafe {
            type Metal = wgpu::wgc::api::Metal;
            let device = self
                .0
                .device
                .as_hal::<Metal>()
                .expect("checked in Renderer::new");
            let queue = self
                .0
                .queue
                .as_hal::<Metal>()
                .expect("the queue is the device's");
            let texture = target
                .texture
                .as_hal::<Metal>()
                .expect("made on the device");
            player.set_metal_target(
                std::ptr::from_ref(&**device.raw_device()).cast(),
                std::ptr::from_ref(queue.as_raw()).cast(),
                std::ptr::from_ref(texture.raw_handle()).cast(),
            );
        }
        let side = |value: u32| {
            i32::try_from(value).expect("texture sides are at most the device's limit")
        };
        player.set_surface_size(side(target.size.0), side(target.size.1));
    }
}

struct Target {
    texture: wgpu::Texture,
    size: (u32, u32),
    /// The D3D11 texture MDK draws into, which backs `texture`; `None` only on the D3D12 path.
    #[cfg(windows)]
    shared: Option<windows::Win32::Graphics::Direct3D11::ID3D11Texture2D>,
    /// MDK's framebuffer over the texture's memory.
    #[cfg(target_os = "linux")]
    gl: crate::opengl::Target,
}

/// A texture and the player that draws into it.
///
/// Every media gets a new player: setting other media on a player that played can block the caller
/// for good, and a stopped one often never draws the new media's first frame.
pub struct Surface {
    // In this order: the player lets go of the target, and the renderer's context goes last.
    player: Option<Player>,
    target: Target,
    renderer: Renderer,
}

impl fmt::Debug for Surface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Surface")
            .field("size", &self.target.size)
            .field("loaded", &self.player.is_some())
            .finish_non_exhaustive()
    }
}

impl Surface {
    /// A texture of `size`, at most [`Renderer::max_side`] a side.
    ///
    /// # Errors
    /// When the size is out of range, or the device cannot make the texture.
    pub fn new(renderer: &Renderer, size: (u32, u32)) -> Result<Self, Error> {
        Ok(Self {
            player: None,
            target: renderer.create_target(size)?,
            renderer: renderer.clone(),
        })
    }

    /// Opens `path` paused at `start_seconds`. The texture keeps its contents until the first frame.
    pub fn load(&mut self, path: &str, start_seconds: f64, callbacks: Callbacks) {
        self.unload();
        let player = Player::new(&self.renderer.0.decoders, callbacks);
        // SAFETY: the target outlives the player (field order, `Drop`, and `resize` attaches the new
        // target before the old one drops).
        unsafe { self.renderer.attach(&player, &self.target) };
        player.open(path, start_seconds);
        self.player = Some(player);
    }

    /// Releases the media. The texture keeps its contents.
    pub fn unload(&mut self) {
        let Some(player) = self.player.take() else {
            return;
        };
        // MDK releases its GL resources in whatever context is current.
        #[cfg(target_os = "linux")]
        if let Err(error) = self.renderer.0.bridge.make_current() {
            log::error!("playback: releasing the player without its context: {error}");
        }
        drop(player);
    }

    #[must_use]
    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    /// A new texture of `size`, at most [`Renderer::max_side`] a side. The current frame is drawn
    /// into it at the next [`Surface::render`].
    ///
    /// # Errors
    /// As [`Surface::new`]; the surface keeps its texture.
    pub fn resize(&mut self, size: (u32, u32)) -> Result<(), Error> {
        let target = self.renderer.create_target(size)?;
        if let Some(player) = &self.player {
            // SAFETY: the new target is stored below and lives as long as the player.
            unsafe { self.renderer.attach(player, &target) };
        }
        self.target = target;
        Ok(())
    }

    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        self.target.size
    }

    /// The texture: `Rgba8Unorm`, sampled with `TEXTURE_BINDING`. Its contents are undefined until
    /// [`Surface::render`] first drew a frame into it.
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.target.texture
    }

    /// Draws the next or current frame: its media second, or `None` without one. Until the media
    /// is loaded nothing is drawn, so the last frame stays in the texture.
    pub fn render(&self) -> Option<f64> {
        let player = self.player.as_ref()?;
        let media_start = player.media_start_seconds()?;
        #[cfg(windows)]
        let ready = self.renderer.0.d3d11.as_ref().map_or(Ok(()), crate::d3d11::Device::before_render);
        #[cfg(target_os = "linux")]
        let ready = self.renderer.0.bridge.before_render(&self.target.gl);
        #[cfg(not(target_os = "macos"))]
        if let Err(error) = ready {
            log::error!("playback: no frame drawn: {error}");
            return None;
        }
        let timestamp = player.render();
        #[cfg(windows)]
        if let Some(Err(error)) = self.renderer.0.d3d11.as_ref().map(crate::d3d11::Device::after_render) {
            log::error!("playback: the frame may not reach the screen: {error}");
        }
        #[cfg(target_os = "linux")]
        self.renderer.0.bridge.after_render(&self.target.gl);
        // MDK stamps frames on the container's clock; seeks and cuts count from the media's start.
        (timestamp >= 0.0).then_some(timestamp - media_start)
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.unload();
    }
}
