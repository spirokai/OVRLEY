//! The Windows target: a D3D11 device on the host's DX12 adapter, whose textures the DX12 device
//! opens through NT handles. MDK's D3D11 decoder and renderer keep each frame on that device. One
//! fence, shared by both, orders the two queues on the GPU around each `renderVideo()`; the render
//! thread never waits.

use crate::{Error, Kind};
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

/// `map_err` for a failed Direct3D call.
fn failed(operation: &'static str) -> impl FnOnce(windows::core::Error) -> Error {
    move |source| Kind::Direct3d { operation, source }.into()
}

pub(crate) struct Device {
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
fn open_shared<T: Interface>(device12: &ID3D12Device, handle: HANDLE) -> windows::core::Result<T> {
    let mut opened: Option<T> = None;
    // SAFETY: `handle` is a shared handle this process created.
    let result = unsafe { device12.OpenSharedHandle(handle, &raw mut opened) };
    // SAFETY: the handle is ours and not used again.
    unsafe { CloseHandle(handle) }?;
    result?;
    Ok(opened.expect("OpenSharedHandle succeeded"))
}

impl Device {
    pub(crate) fn new(wgpu_device: &wgpu::Device, wgpu_queue: &wgpu::Queue) -> Result<Self, Error> {
        // SAFETY: the handles are only cloned; nothing is destroyed through them.
        let device12 = unsafe { wgpu_device.as_hal::<Dx12>() }
            .ok_or(Kind::Backend("DX12"))?
            .raw_device()
            .clone();
        // SAFETY: as above.
        let queue = unsafe { wgpu_queue.as_hal::<Dx12>() }
            .ok_or(Kind::Backend("DX12"))?
            .as_raw()
            .clone();
        // SAFETY: plain object creation and queries.
        let factory: IDXGIFactory4 =
            unsafe { CreateDXGIFactory1() }.map_err(failed("CreateDXGIFactory1"))?;
        // SAFETY: as above.
        let adapter: IDXGIAdapter = unsafe { factory.EnumAdapterByLuid(device12.GetAdapterLuid()) }
            .map_err(failed("finding the DX12 device's adapter"))?;

        let mut device = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        // SAFETY: the out parameters are written on success.
        unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1]),
                D3D11_SDK_VERSION,
                Some(&raw mut device),
                None,
                Some(&raw mut context),
            )
        }
        .map_err(failed("D3D11CreateDevice"))?;
        let device: ID3D11Device = device.expect("D3D11CreateDevice succeeded");
        let context: ID3D11DeviceContext4 = context
            .expect("D3D11CreateDevice succeeded")
            .cast()
            .map_err(failed("ID3D11DeviceContext4"))?;
        // MDK's decoder threads share the immediate context with renderVideo() on the render thread.
        let multithread = device
            .cast::<ID3D11Multithread>()
            .map_err(failed("ID3D11Multithread"))?;
        // SAFETY: a plain setting; the previous value it returns is not needed.
        let _ = unsafe { multithread.SetMultithreadProtected(true) };

        let mut fence11: Option<ID3D11Fence> = None;
        let device5 = device
            .cast::<ID3D11Device5>()
            .map_err(failed("ID3D11Device5"))?;
        // SAFETY: the out parameter is written on success.
        unsafe { device5.CreateFence(0, D3D11_FENCE_FLAG_SHARED, &raw mut fence11) }
            .map_err(failed("creating the shared fence"))?;
        let fence11 = fence11.expect("CreateFence succeeded");
        // SAFETY: a shared fence may be shared.
        let handle = unsafe { fence11.CreateSharedHandle(None, GENERIC_ALL.0, PCWSTR::null()) }
            .map_err(failed("sharing the fence"))?;
        let fence12: ID3D12Fence =
            open_shared(&device12, handle).map_err(failed("opening the fence on DX12"))?;
        log::debug!("playback: D3D11 target device on the shared adapter, fence shared with DX12");
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

    /// A D3D11 render target MDK draws into, and the same memory as a wgpu texture the host samples.
    pub(crate) fn create_target(
        &self,
        size: (u32, u32),
    ) -> Result<(ID3D11Texture2D, wgpu::Texture), Error> {
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
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0).cast_unsigned(),
            CPUAccessFlags: 0,
            MiscFlags: (D3D11_RESOURCE_MISC_SHARED.0 | D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0)
                .cast_unsigned(),
        };
        let mut texture = None;
        // SAFETY: a valid description; the caller checked the size against the device's limit.
        unsafe {
            self.device
                .CreateTexture2D(&desc, None, Some(&raw mut texture))
        }
        .map_err(failed("creating the frame texture"))?;
        let texture: ID3D11Texture2D = texture.expect("CreateTexture2D succeeded");
        let resource1 = texture
            .cast::<IDXGIResource1>()
            .map_err(failed("IDXGIResource1"))?;
        // SAFETY: the texture was created shareable through an NT handle.
        let handle = unsafe {
            resource1.CreateSharedHandle(
                None,
                (DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE).0,
                PCWSTR::null(),
            )
        }
        .map_err(failed("sharing the frame texture"))?;
        let resource: ID3D12Resource = open_shared(&self.device12, handle)
            .map_err(failed("opening the frame texture on DX12"))?;

        let extent = wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        };
        // SAFETY: the resource is a 2D Rgba8Unorm texture of `extent`, one mip and one sample.
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
        // SAFETY: opened on this device, as described. The host only samples it. A shared texture
        // starts in COMMON, which D3D12 promotes to a shader-read state implicitly and decays back
        // after each submission, so wgpu's tracked state (sampled) never needs a barrier that would
        // hold it against D3D11.
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

    /// Before MDK draws: D3D11 waits on the GPU for what DX12 already submitted (the host's last
    /// frame, which samples the texture MDK is about to overwrite). Fails when the device was
    /// removed; MDK must not draw then.
    pub(crate) fn before_render(&self) -> Result<(), Error> {
        let value = self.value.get() + 1;
        self.value.set(value);
        // SAFETY: both fences are the shared pair; values only grow.
        unsafe {
            self.queue
                .Signal(&self.fence12, value)
                .map_err(failed("signalling the shared fence on DX12"))?;
            self.context
                .Wait(&self.fence11, value)
                .map_err(failed("waiting on the shared fence on D3D11"))
        }
    }

    /// After MDK drew: DX12 waits on the GPU for D3D11's draw before the host's next frame samples
    /// it.
    pub(crate) fn after_render(&self) -> Result<(), Error> {
        let value = self.value.get() + 1;
        self.value.set(value);
        // SAFETY: as in `before_render`. DX12 waits only on a value D3D11 was told to signal.
        unsafe {
            self.context
                .Signal(&self.fence11, value)
                .map_err(failed("signalling the shared fence on D3D11"))?;
            self.context.Flush();
            self.queue
                .Wait(&self.fence12, value)
                .map_err(failed("waiting on the shared fence on DX12"))
        }
    }
}
