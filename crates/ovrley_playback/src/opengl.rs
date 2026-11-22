//! The Linux target: MDK renders with OpenGL, the host's device is Vulkan. The frame stays on the
//! GPU:
//! - Vulkan allocates the frame texture with exportable memory (`VK_KHR_external_memory_fd`);
//!   an OpenGL context on the same GPU imports it (`GL_EXT_memory_object_fd`) as the colour
//!   attachment of the framebuffer MDK draws into, and wgpu wraps the same image for the host.
//! - Two binary semaphores shared the same way (`VK_KHR_external_semaphore_fd`,
//!   `GL_EXT_semaphore_fd`) order the two queues on the GPU around each `renderVideo()`; the render
//!   thread never waits.
//!
//! The context is an EGL context without a surface, on the EGL device whose OpenGL device UUID
//! is the Vulkan device's (`EGL_EXT_platform_device`), so it works with any vendor's driver and
//! picks the right GPU on hybrid systems. It is made current on the render thread before MDK draws
//! or releases its resources there, because MDK uses whatever context is current.
//!
//! The image never changes queue family ownership to `VK_QUEUE_FAMILY_EXTERNAL`: wgpu cannot
//! record that barrier. Drivers that share memory between their own GL and Vulkan (Mesa, NVIDIA)
//! do not need it for an exclusive image used by one queue on each side.

use crate::{Error, Kind};
use ash::vk;
use khronos_egl as egl;
use std::ffi::{c_void, CStr};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
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
const MEMORY_HANDLE: vk::ExternalMemoryHandleTypeFlags =
    vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD;
const SEMAPHORE_HANDLE: vk::ExternalSemaphoreHandleTypeFlags =
    vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD;

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
    #[expect(clippy::missing_transmute_annotations, reason = "the struct field names the type")]
    fn load(egl: &Egl) -> Result<Self, Error> {
        macro_rules! load {
            ($name:literal) => {{
                let function = egl.get_proc_address($name).ok_or(Kind::GlFunction($name))?;
                // SAFETY: the field's type is the entry point's signature in the GL registry.
                unsafe {
                    std::mem::transmute::<extern "system" fn(), _>(function)
                }
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

    /// A `glGetString` value, `""` when there is none. The context is current.
    fn string(&self, name: u32) -> String {
        // SAFETY: a query on the current context.
        let value = unsafe { (self.get_string)(name) };
        if value.is_null() {
            return String::new();
        }
        // SAFETY: GL returns a static C string.
        unsafe { CStr::from_ptr(value.cast()) }
            .to_string_lossy()
            .into_owned()
    }

    /// The context is current.
    fn has_extension(&self, name: &str) -> bool {
        let mut count = 0;
        // SAFETY: a query on the current context.
        unsafe { (self.get_integerv)(GL_NUM_EXTENSIONS, &raw mut count) };
        (0..count.max(0).unsigned_abs()).any(|index| {
            // SAFETY: `index` is below `GL_NUM_EXTENSIONS`.
            let value = unsafe { (self.get_stringi)(GL_EXTENSIONS, index) };
            // SAFETY: GL returns a static C string.
            !value.is_null()
                && unsafe { CStr::from_ptr(value.cast()) }.to_bytes() == name.as_bytes()
        })
    }

    /// The UUIDs of the devices behind the current context.
    fn device_uuids(&self) -> Vec<[u8; vk::UUID_SIZE]> {
        let mut count = 0;
        // SAFETY: a query on the current context.
        unsafe { (self.get_integerv)(GL_NUM_DEVICE_UUIDS_EXT, &raw mut count) };
        (0..count.max(0).unsigned_abs())
            .map(|index| {
                let mut uuid = [0; vk::UUID_SIZE];
                // SAFETY: `index` is below `GL_NUM_DEVICE_UUIDS_EXT`; GL writes `GL_UUID_SIZE_EXT`
                // (16) bytes.
                unsafe {
                    (self.get_unsigned_bytei_v)(GL_DEVICE_UUID_EXT, index, uuid.as_mut_ptr())
                };
                uuid
            })
            .collect()
    }

    /// The first error since the last check, on the current context.
    fn check(&self, operation: &'static str) -> Result<(), Error> {
        // SAFETY: a query on the current context.
        match unsafe { (self.get_error)() } {
            0 => Ok(()),
            code => Err(Kind::Gl { operation, code }.into()),
        }
    }
}

/// An EGL context on the Vulkan device's GPU.
struct Context {
    egl: Egl,
    display: egl::Display,
    context: egl::Context,
    gl: Gl,
}

impl Context {
    /// The first EGL device (then the default display) whose OpenGL context runs on the device
    /// with `uuid` and can import Vulkan memory and semaphores. It is current on return.
    fn on_device(uuid: [u8; vk::UUID_SIZE]) -> Result<Self, Error> {
        // SAFETY: libEGL.so.1 is the EGL ABI (glvnd's dispatcher or a vendor's library).
        let egl =
            unsafe { Egl::load_required() }.map_err(|error| Kind::LoadEgl(error.to_string()))?;
        let client = egl
            .query_string(None, egl::EXTENSIONS)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let client: Vec<&str> = client.split_whitespace().collect();
        let mut displays = Vec::new();
        if client.contains(&"EGL_EXT_device_enumeration")
            && client.contains(&"EGL_EXT_platform_device")
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
        Err(Kind::NoDisplay(rejected).into())
    }

    /// A current context on `display`, or why the display does not serve.
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
            if !extensions
                .split_whitespace()
                .any(|extension| extension == needed)
            {
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
        let gl = Gl::load(egl).map_err(|error| reject_context(error.to_string()))?;
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

    fn make_current(&self) -> Result<(), Error> {
        self.egl
            .make_current(self.display, None, None, Some(self.context))
            .map_err(|source| {
                Kind::Egl {
                    operation: "eglMakeCurrent",
                    source,
                }
                .into()
            })
    }
}

impl Drop for Context {
    /// The display stays initialized: EGL counts no references to it, so terminating it would
    /// destroy every other context on it, a second bridge's among them.
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.context);
    }
}

/// The EGL devices (`eglQueryDevicesEXT`).
fn query_devices(egl: &Egl) -> Vec<*mut c_void> {
    type QueryDevices = unsafe extern "system" fn(i32, *mut *mut c_void, *mut i32) -> u32;
    let Some(function) = egl.get_proc_address("eglQueryDevicesEXT") else {
        return Vec::new();
    };
    // SAFETY: eglQueryDevicesEXT's signature (EGL_EXT_device_enumeration).
    let query = unsafe { std::mem::transmute::<extern "system" fn(), QueryDevices>(function) };
    let mut count = 0;
    // SAFETY: a null array asks for the count.
    if unsafe { query(0, std::ptr::null_mut(), &raw mut count) } == 0 || count <= 0 {
        return Vec::new();
    }
    let mut devices = vec![std::ptr::null_mut(); count.unsigned_abs() as usize];
    // SAFETY: `devices` holds `count` entries.
    if unsafe { query(count, devices.as_mut_ptr(), &raw mut count) } == 0 {
        return Vec::new();
    }
    devices.truncate(count.max(0).unsigned_abs() as usize);
    devices
}

/// A binary semaphore created on Vulkan and imported into OpenGL.
struct SharedSemaphore {
    vulkan: vk::Semaphore,
    gl: u32,
    context: Rc<Context>,
    raw_device: ash::Device,
    /// Keeps the Vulkan device alive for `raw_device`.
    _device: wgpu::Device,
}

impl SharedSemaphore {
    /// The context is current.
    fn new(
        device: &wgpu::Device,
        raw_device: &ash::Device,
        semaphore_fd: &ash::khr::external_semaphore_fd::Device,
        context: &Rc<Context>,
        operation: &'static str,
    ) -> Result<Self, Error> {
        let mut export = vk::ExportSemaphoreCreateInfo::default().handle_types(SEMAPHORE_HANDLE);
        let info = vk::SemaphoreCreateInfo::default().push_next(&mut export);
        // SAFETY: a valid create info on a live device.
        let vulkan =
            unsafe { raw_device.create_semaphore(&info, None) }.map_err(|source| Kind::Vulkan {
                operation: "vkCreateSemaphore",
                source,
            })?;
        let mut semaphore = Self {
            vulkan,
            gl: 0,
            context: Rc::clone(context),
            raw_device: raw_device.clone(),
            _device: device.clone(),
        };
        let info = vk::SemaphoreGetFdInfoKHR::default()
            .semaphore(vulkan)
            .handle_type(SEMAPHORE_HANDLE);
        // SAFETY: the semaphore was created exportable as an opaque fd.
        let fd =
            unsafe { semaphore_fd.get_semaphore_fd(&info) }.map_err(|source| Kind::Vulkan {
                operation: "vkGetSemaphoreFdKHR",
                source,
            })?;
        // SAFETY: a new descriptor that nothing else owns.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let gl = &context.gl;
        // SAFETY: calls on the current context; the name is written before it is used.
        unsafe { (gl.gen_semaphores)(1, &raw mut semaphore.gl) };
        gl.check(operation)?;
        // SAFETY: as above; `fd` is an opaque semaphore descriptor.
        unsafe {
            (gl.import_semaphore_fd)(semaphore.gl, GL_HANDLE_TYPE_OPAQUE_FD_EXT, fd.as_raw_fd())
        };
        gl.check(operation)?;
        // OpenGL owns the descriptor after a successful import.
        let _ = fd.into_raw_fd();
        Ok(semaphore)
    }
}

impl Drop for SharedSemaphore {
    /// Neither queue may still use it.
    fn drop(&mut self) {
        match self.context.make_current() {
            // SAFETY: the name is this semaphore's (0, before the import, is ignored).
            Ok(()) => unsafe { (self.context.gl.delete_semaphores)(1, &raw const self.gl) },
            Err(error) => log::error!("playback: leaking an OpenGL semaphore: {error}"),
        }
        // SAFETY: no queue uses it any more (the bridge waited for both).
        unsafe { self.raw_device.destroy_semaphore(self.vulkan, None) };
    }
}

/// What all targets share: the context, and the semaphores that order the two queues.
pub(crate) struct Bridge {
    device: wgpu::Device,
    queue: wgpu::Queue,
    raw_device: ash::Device,
    memory_types: vk::PhysicalDeviceMemoryProperties,
    memory_fd: ash::khr::external_memory_fd::Device,
    /// Vulkan signals it after its last use of the frame (the host's last draw sampling it);
    /// OpenGL waits on it before MDK draws.
    to_gl: SharedSemaphore,
    /// OpenGL signals it after MDK drew; Vulkan waits on it before the host's next draw samples.
    to_vulkan: SharedSemaphore,
    context: Rc<Context>,
}

/// The frame texture's OpenGL side: MDK's framebuffer, over the memory Vulkan exported.
pub(crate) struct Target {
    context: Rc<Context>,
    texture: u32,
    memory: u32,
    fbo: u32,
}

impl Target {
    pub(crate) fn fbo(&self) -> u32 {
        self.fbo
    }
}

impl Drop for Target {
    /// OpenGL defers the deletes until its queued work is done; Vulkan's side is wgpu's texture.
    fn drop(&mut self) {
        if let Err(error) = self.context.make_current() {
            log::error!("playback: leaking a frame's OpenGL objects: {error}");
            return;
        }
        let gl = &self.context.gl;
        // SAFETY: the names are this target's (0, for one never made, is ignored).
        unsafe {
            (gl.delete_framebuffers)(1, &raw const self.fbo);
            (gl.delete_textures)(1, &raw const self.texture);
            (gl.delete_memory_objects)(1, &raw const self.memory);
        }
    }
}

impl Bridge {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, Error> {
        // SAFETY: the device's handles are only read and cloned; nothing is destroyed through them.
        let hal_device = unsafe { device.as_hal::<Vulkan>() }.ok_or(Kind::Backend("Vulkan"))?;
        for needed in [
            ash::khr::external_memory_fd::NAME,
            ash::khr::external_semaphore_fd::NAME,
        ] {
            if !hal_device.enabled_device_extensions().contains(&needed) {
                return Err(Kind::VulkanExtension(needed.to_string_lossy().into_owned()).into());
            }
        }
        let instance = hal_device.shared_instance().raw_instance();
        let physical = hal_device.raw_physical_device();
        let raw_device = hal_device.raw_device().clone();
        let mut id = vk::PhysicalDeviceIDProperties::default();
        let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
        // SAFETY: the physical device is the device's.
        unsafe { instance.get_physical_device_properties2(physical, &mut properties) };
        let uuid = id.device_uuid;
        // SAFETY: as above.
        let memory_types = unsafe { instance.get_physical_device_memory_properties(physical) };
        let memory_fd = ash::khr::external_memory_fd::Device::new(instance, &raw_device);
        let semaphore_fd = ash::khr::external_semaphore_fd::Device::new(instance, &raw_device);
        drop(hal_device);

        let context = Rc::new(Context::on_device(uuid)?);
        let share = |operation| {
            SharedSemaphore::new(device, &raw_device, &semaphore_fd, &context, operation)
        };
        let to_gl = share("importing the Vulkan → OpenGL semaphore")?;
        let to_vulkan = share("importing the OpenGL → Vulkan semaphore")?;
        log::debug!("playback: Vulkan–OpenGL bridge with shared memory and semaphores");
        Ok(Self {
            device: device.clone(),
            queue: queue.clone(),
            raw_device,
            memory_types,
            memory_fd,
            to_gl,
            to_vulkan,
            context,
        })
    }

    /// Makes the bridge's context current on this thread, for MDK.
    pub(crate) fn make_current(&self) -> Result<(), Error> {
        self.context.make_current()
    }

    /// A Vulkan image with exportable memory as a wgpu texture the host samples, and its memory
    /// imported into OpenGL as MDK's framebuffer. The image is handed over once, from OpenGL to
    /// Vulkan, so that it is in the layout wgpu is told about before anyone can sample it.
    pub(crate) fn create_target(&self, size: (u32, u32)) -> Result<(Target, wgpu::Texture), Error> {
        let device = &self.raw_device;
        let mut external = vk::ExternalMemoryImageCreateInfo::default().handle_types(MEMORY_HANDLE);
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
        // SAFETY: a valid create info; the caller checked the size against the device's limit.
        let image = unsafe { device.create_image(&info, None) }.map_err(|source| Kind::Vulkan {
            operation: "vkCreateImage",
            source,
        })?;
        // SAFETY: the image is this device's.
        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type = self
            .memory_types
            .memory_types_as_slice()
            .iter()
            .zip(0u32..)
            .find_map(|(kind, index)| {
                let allowed = requirements.memory_type_bits & (1 << index) != 0;
                (allowed
                    && kind
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL))
                .then_some(index)
            });
        let Some(memory_type) = memory_type else {
            // SAFETY: unused and unbound.
            unsafe { device.destroy_image(image, None) };
            return Err(Kind::MemoryType.into());
        };
        // Dedicated: OpenGL imports it as a dedicated memory object, which must match.
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let mut export = vk::ExportMemoryAllocateInfo::default().handle_types(MEMORY_HANDLE);
        let allocate = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type)
            .push_next(&mut dedicated)
            .push_next(&mut export);
        // SAFETY: a valid allocate info for the image.
        let memory = match unsafe { device.allocate_memory(&allocate, None) } {
            Ok(memory) => memory,
            Err(source) => {
                // SAFETY: unused and unbound.
                unsafe { device.destroy_image(image, None) };
                return Err(Kind::Vulkan {
                    operation: "vkAllocateMemory",
                    source,
                }
                .into());
            }
        };
        // SAFETY: the memory was allocated for the image, exportable as an opaque fd.
        let fd = unsafe { device.bind_image_memory(image, memory, 0) }.and_then(|()| unsafe {
            let info = vk::MemoryGetFdInfoKHR::default()
                .memory(memory)
                .handle_type(MEMORY_HANDLE);
            self.memory_fd.get_memory_fd(&info)
        });
        let fd = match fd {
            // SAFETY: a new descriptor that nothing else owns.
            Ok(fd) => unsafe { OwnedFd::from_raw_fd(fd) },
            Err(source) => {
                // SAFETY: unused by any queue.
                unsafe {
                    device.destroy_image(image, None);
                    device.free_memory(memory, None);
                }
                return Err(Kind::Vulkan {
                    operation: "exporting the frame's memory",
                    source,
                }
                .into());
            }
        };

        // wgpu owns the image and its memory from here, and frees them once the GPU is done.
        let extent = wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        };
        let hal_texture = {
            // SAFETY: the device is only used to wrap the image.
            let hal_device =
                unsafe { self.device.as_hal::<Vulkan>() }.expect("checked in Bridge::new");
            // SAFETY: the image was created as the descriptor says, without MUTABLE_FORMAT; wgpu
            // takes the image and its dedicated memory.
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
        // SAFETY: made on this device as described. The host only samples it, in the layout OpenGL
        // leaves it in, which the hand-over below establishes before the texture is returned.
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

        self.context.make_current()?;
        let gl = &self.context.gl;
        let mut target = Target {
            context: Rc::clone(&self.context),
            texture: 0,
            memory: 0,
            fbo: 0,
        };
        let side = |value: u32| {
            i32::try_from(value).expect("texture sides are at most the device's limit")
        };
        // SAFETY: calls on the current context, with names it made; `fd` is the image's opaque
        // memory descriptor, of `requirements.size` bytes.
        unsafe {
            (gl.create_memory_objects)(1, &raw mut target.memory);
            (gl.memory_object_parameteriv)(target.memory, GL_DEDICATED_MEMORY_OBJECT_EXT, &1);
            (gl.import_memory_fd)(
                target.memory,
                requirements.size,
                GL_HANDLE_TYPE_OPAQUE_FD_EXT,
                fd.as_raw_fd(),
            );
        }
        gl.check("importing the frame's memory")?;
        // OpenGL owns the descriptor after a successful import.
        let _ = fd.into_raw_fd();
        // SAFETY: as above.
        unsafe {
            (gl.gen_textures)(1, &raw mut target.texture);
            (gl.bind_texture)(GL_TEXTURE_2D, target.texture);
            #[expect(clippy::cast_possible_wrap, reason = "a GLenum passed as GLint")]
            (gl.tex_parameteri)(
                GL_TEXTURE_2D,
                GL_TEXTURE_TILING_EXT,
                GL_OPTIMAL_TILING_EXT as i32,
            );
            (gl.tex_storage_mem_2d)(
                GL_TEXTURE_2D,
                1,
                GL_RGBA8,
                side(size.0),
                side(size.1),
                target.memory,
                0,
            );
            (gl.bind_texture)(GL_TEXTURE_2D, 0);
            (gl.gen_framebuffers)(1, &raw mut target.fbo);
            (gl.bind_framebuffer)(GL_FRAMEBUFFER, target.fbo);
            (gl.framebuffer_texture_2d)(
                GL_FRAMEBUFFER,
                GL_COLOR_ATTACHMENT0,
                GL_TEXTURE_2D,
                target.texture,
                0,
            );
        }
        // SAFETY: as above.
        let status = unsafe { (gl.check_framebuffer_status)(GL_FRAMEBUFFER) };
        // SAFETY: as above.
        unsafe { (gl.bind_framebuffer)(GL_FRAMEBUFFER, 0) };
        gl.check("making the frame's framebuffer")?;
        if status != GL_FRAMEBUFFER_COMPLETE {
            return Err(Kind::Framebuffer(status).into());
        }
        self.hand_to_gl(&target, GL_LAYOUT_UNDEFINED)?;
        self.hand_to_vulkan(&target);
        Ok((target, texture))
    }

    /// Before MDK draws: OpenGL waits for what Vulkan already submitted.
    pub(crate) fn before_render(&self, target: &Target) -> Result<(), Error> {
        self.hand_to_gl(target, GL_LAYOUT_SHADER_READ_ONLY_EXT)
    }

    /// After MDK drew, on the context [`Bridge::before_render`] made current: Vulkan waits for
    /// OpenGL's draw.
    pub(crate) fn after_render(&self, target: &Target) {
        self.hand_to_vulkan(target);
    }

    /// OpenGL waits on the GPU for what Vulkan already submitted (the host's last frame, which
    /// samples the texture MDK is about to overwrite), finding the image in `layout`. The host
    /// submits on the same VkQueue as wgpu, so the signal, in a submission of its own, follows the
    /// host's work too. Leaves the context current.
    fn hand_to_gl(&self, target: &Target, layout: u32) -> Result<(), Error> {
        // Before the signal: a signal nobody waits on would be signalled twice next time.
        self.context.make_current()?;
        // SAFETY: the semaphore is the device's; the submission below consumes it.
        unsafe { self.queue.as_hal::<Vulkan>() }
            .expect("the queue is the bridge device's")
            .add_signal_semaphore(self.to_gl.vulkan, None);
        self.queue.submit([]);
        // SAFETY: on the current context, with its semaphore and texture names.
        unsafe {
            (self.context.gl.wait_semaphore)(
                self.to_gl.gl,
                0,
                std::ptr::null(),
                1,
                &raw const target.texture,
                &raw const layout,
            )
        };
        Ok(())
    }

    /// Vulkan waits on the GPU for OpenGL's work before the host's next frame samples the image,
    /// which OpenGL leaves shader-read-only. The wait goes in a submission of its own: a semaphore
    /// wait also holds back everything submitted after it on the queue, the host's draws included.
    /// The context is current.
    fn hand_to_vulkan(&self, target: &Target) {
        let layout = GL_LAYOUT_SHADER_READ_ONLY_EXT;
        let gl = &self.context.gl;
        // SAFETY: on the current context, with its semaphore and texture names.
        unsafe {
            (gl.signal_semaphore)(
                self.to_vulkan.gl,
                0,
                std::ptr::null(),
                1,
                &raw const target.texture,
                &raw const layout,
            );
            // Submits the signal, which Vulkan's wait below must follow.
            (gl.flush)();
        }
        // SAFETY: the semaphore is the device's; the submission below consumes it.
        unsafe { self.queue.as_hal::<Vulkan>() }
            .expect("the queue is the bridge device's")
            .add_wait_semaphore(
                self.to_vulkan.vulkan,
                None,
                vk::PipelineStageFlags::ALL_COMMANDS,
            );
        self.queue.submit([]);
    }
}

impl Drop for Bridge {
    /// Neither queue may still use the semaphores, which drop after this.
    fn drop(&mut self) {
        match self.context.make_current() {
            // SAFETY: on the current context.
            Ok(()) => unsafe { (self.context.gl.finish)() },
            Err(error) => log::error!("playback: the bridge's context is gone: {error}"),
        }
        if let Err(error) = self.device.poll(wgpu::PollType::wait_indefinitely()) {
            log::error!("playback: waiting for the GPU before the bridge's teardown: {error}");
        }
    }
}
