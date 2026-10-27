#include "void/rhi/RHI.h"

#include "void/core/Log.h"

#include <SDL3/SDL.h>
#include <d3d12.h>
#include <dxgi1_6.h>
#include <wrl/client.h>

#include <array>
#include <string>

using Microsoft::WRL::ComPtr;

namespace vd::rhi {
namespace {

constexpr u32 kFramesInFlight = 2;
constexpr DXGI_FORMAT kSwapchainFormat = DXGI_FORMAT_B8G8R8A8_UNORM;
constexpr DXGI_FORMAT kDepthFormat = DXGI_FORMAT_D32_FLOAT;

void check(HRESULT hr, const char* what) {
    if (FAILED(hr)) {
        log::error("DX12 %s failed: 0x%08x", what, static_cast<unsigned>(hr));
    }
}

class DX12Device final : public Device {
public:
    ~DX12Device() override { shutdown(); }

    bool initialize(const DeviceDesc& desc) override {
        m_width = desc.width;
        m_height = desc.height;
        m_vsync = desc.vsync;

        UINT factoryFlags = 0;
        if (desc.validation) {
            ComPtr<ID3D12Debug> debug;
            if (SUCCEEDED(D3D12GetDebugInterface(IID_PPV_ARGS(&debug)))) {
                debug->EnableDebugLayer();
                m_debug = true;
                factoryFlags |= DXGI_CREATE_FACTORY_DEBUG;
            }
        }

        check(CreateDXGIFactory2(factoryFlags, IID_PPV_ARGS(&m_factory)), "CreateDXGIFactory2");
        if (!m_factory) return false;

        if (!selectAdapter(desc.adapter)) return false;

        DXGI_ADAPTER_DESC1 adapterDesc{};
        m_adapter->GetDesc1(&adapterDesc);
        m_adapterName = toUtf8(adapterDesc.Description);

        check(D3D12CreateDevice(m_adapter.Get(), D3D_FEATURE_LEVEL_12_0, IID_PPV_ARGS(&m_device)), "D3D12CreateDevice");
        if (!m_device) return false;

        D3D12_COMMAND_QUEUE_DESC queueDesc{};
        queueDesc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
        check(m_device->CreateCommandQueue(&queueDesc, IID_PPV_ARGS(&m_queue)), "CreateCommandQueue");
        if (!m_queue) return false;

        if (!createDescriptorHeaps()) return false;
        if (!createSwapchain(desc.window)) return false;
        if (!createDepthBuffer()) return false;

        for (u32 i = 0; i < kFramesInFlight; ++i) {
            check(m_device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&m_allocators[i])),
                  "CreateCommandAllocator");
            m_frameFenceValues[i] = 0;
        }
        check(m_device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, m_allocators[0].Get(), nullptr,
                                          IID_PPV_ARGS(&m_commandList)), "CreateCommandList");
        m_commandList->Close();

        check(m_device->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_PPV_ARGS(&m_fence)), "CreateFence");
        m_fenceEvent = CreateEventW(nullptr, FALSE, FALSE, nullptr);
        if (!m_fenceEvent) {
            log::error("CreateEventW failed");
            return false;
        }

        log::info("DX12 adapter: %s", m_adapterName.c_str());
        log::info("DX12 validation: %s", m_debug ? "on" : "off");
        return true;
    }

    void shutdown() override {
        if (!m_device) return;
        waitIdle();
        if (m_fenceEvent) {
            CloseHandle(m_fenceEvent);
            m_fenceEvent = nullptr;
        }
        m_commandList.Reset();
        for (auto& allocator : m_allocators) allocator.Reset();
        m_depth.Reset();
        m_dsvHeap.Reset();
        for (auto& target : m_renderTargets) target.Reset();
        m_rtvHeap.Reset();
        m_swapchain.Reset();
        m_queue.Reset();
        m_device.Reset();
        m_adapter.Reset();
        m_factory.Reset();
    }

    void resize(u32 width, u32 height) override {
        if (!m_swapchain || width == 0 || height == 0) return;
        if (width == m_width && height == m_height) return;
        waitIdle();

        for (auto& target : m_renderTargets) target.Reset();
        m_depth.Reset();

        m_width = width;
        m_height = height;
        DXGI_SWAP_CHAIN_DESC1 desc{};
        desc.Width = width;
        desc.Height = height;
        desc.Format = kSwapchainFormat;
        desc.SampleDesc.Count = 1;
        desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
        desc.BufferCount = static_cast<UINT>(m_renderTargets.size());
        desc.Scaling = DXGI_SCALING_STRETCH;
        desc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_DISCARD;
        desc.AlphaMode = DXGI_ALPHA_MODE_IGNORE;
        desc.Flags = m_vsync ? 0 : DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING;

        HRESULT hr = m_swapchain->ResizeBuffers(0, width, height, DXGI_FORMAT_UNKNOWN, desc.Flags);
        if (FAILED(hr)) {
            log::error("DX12 ResizeBuffers failed: 0x%08x", static_cast<unsigned>(hr));
            return;
        }
        createRenderTargets();
        createDepthBuffer();
    }

    void waitIdle() override {
        if (!m_queue || !m_fence) return;
        const u64 value = ++m_fenceValue;
        if (SUCCEEDED(m_queue->Signal(m_fence.Get(), value))) {
            m_fence->SetEventOnCompletion(value, m_fenceEvent);
            WaitForSingleObject(m_fenceEvent, INFINITE);
        }
    }

    MeshHandle createMesh(const MeshData&) override { return {}; }

    bool createTextureArray(const std::vector<ImageData>&) override { return false; }

    bool createFontTexture(const ImageData&) override { return false; }

    void render(const FrameData&, const DrawList&, const UIList&) override {
        waitForFrame();

        const u32 index = m_swapchain->GetCurrentBackBufferIndex();
        m_allocators[m_frameSlot]->Reset();
        m_commandList->Reset(m_allocators[m_frameSlot].Get(), nullptr);

        D3D12_RESOURCE_BARRIER barrier{};
        barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        barrier.Transition.pResource = m_renderTargets[index].Get();
        barrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
        barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_PRESENT;
        barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_RENDER_TARGET;
        m_commandList->ResourceBarrier(1, &barrier);

        const D3D12_CPU_DESCRIPTOR_HANDLE rtv = m_rtvHandles[index];
        m_commandList->OMSetRenderTargets(1, &rtv, FALSE, nullptr);

        const float clear[4] = {0.035f, 0.045f, 0.055f, 1.0f};
        m_commandList->ClearRenderTargetView(rtv, clear, 0, nullptr);

        D3D12_RESOURCE_BARRIER presentBarrier{};
        presentBarrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        presentBarrier.Transition.pResource = m_renderTargets[index].Get();
        presentBarrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
        presentBarrier.Transition.StateBefore = D3D12_RESOURCE_STATE_RENDER_TARGET;
        presentBarrier.Transition.StateAfter = D3D12_RESOURCE_STATE_PRESENT;
        m_commandList->ResourceBarrier(1, &presentBarrier);
        m_commandList->Close();

        ID3D12CommandList* lists[] = {m_commandList.Get()};
        m_queue->ExecuteCommandLists(1, lists);

        const UINT syncInterval = m_vsync ? 1 : 0;
        const UINT presentFlags = m_vsync ? 0 : DXGI_PRESENT_ALLOW_TEARING;
        m_swapchain->Present(syncInterval, presentFlags);

        const u64 value = ++m_fenceValue;
        m_queue->Signal(m_fence.Get(), value);
        m_frameFenceValues[m_frameSlot] = value;
        m_frameSlot = (m_frameSlot + 1) % kFramesInFlight;
    }

    Api api() const override { return Api::DX12; }
    const char* adapterName() const override { return m_adapterName.c_str(); }
    const Stats& stats() const override { return m_stats; }

private:
    static std::string toUtf8(const wchar_t* text) {
        if (!text) return {};
        int size = WideCharToMultiByte(CP_UTF8, 0, text, -1, nullptr, 0, nullptr, nullptr);
        if (size <= 1) return {};
        std::string out(static_cast<size_t>(size - 1), '\0');
        WideCharToMultiByte(CP_UTF8, 0, text, -1, out.data(), size, nullptr, nullptr);
        return out;
    }

    bool selectAdapter(int requested) {
        if (requested >= 0) {
            ComPtr<IDXGIAdapter1> candidate;
            if (FAILED(m_factory->EnumAdapters1(static_cast<UINT>(requested), &candidate))) {
                log::error("DX12 adapter %d not found", requested);
                return false;
            }
            m_adapter = candidate;
            return true;
        }

        for (u32 i = 0;; ++i) {
            ComPtr<IDXGIAdapter1> candidate;
            HRESULT hr = m_factory->EnumAdapterByGpuPreference(
                i, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, IID_PPV_ARGS(&candidate));
            if (hr == DXGI_ERROR_NOT_FOUND) break;
            if (FAILED(hr)) break;
            DXGI_ADAPTER_DESC1 desc{};
            candidate->GetDesc1(&desc);
            if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) continue;
            if (SUCCEEDED(D3D12CreateDevice(candidate.Get(), D3D_FEATURE_LEVEL_12_0, __uuidof(ID3D12Device), nullptr))) {
                m_adapter = candidate;
                return true;
            }
        }

        ComPtr<IDXGIFactory1> factory1;
        if (SUCCEEDED(m_factory.As(&factory1))) {
            for (u32 i = 0;; ++i) {
                ComPtr<IDXGIAdapter1> candidate;
                if (factory1->EnumAdapters1(i, &candidate) == DXGI_ERROR_NOT_FOUND) break;
                DXGI_ADAPTER_DESC1 desc{};
                candidate->GetDesc1(&desc);
                if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) continue;
                if (SUCCEEDED(D3D12CreateDevice(candidate.Get(), D3D_FEATURE_LEVEL_12_0, __uuidof(ID3D12Device), nullptr))) {
                    m_adapter = candidate;
                    return true;
                }
            }
        }

        log::error("No D3D12 12_0 adapter found");
        return false;
    }

    bool createSwapchain(SDL_Window* window) {
        HWND hwnd = static_cast<HWND>(SDL_GetPointerProperty(
            SDL_GetWindowProperties(window), SDL_PROP_WINDOW_WIN32_HWND_POINTER, nullptr));
        if (!hwnd) {
            log::error("DX12: window has no HWND");
            return false;
        }

        DXGI_SWAP_CHAIN_DESC1 desc{};
        desc.Width = m_width;
        desc.Height = m_height;
        desc.Format = kSwapchainFormat;
        desc.SampleDesc.Count = 1;
        desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
        desc.BufferCount = static_cast<UINT>(m_renderTargets.size());
        desc.Scaling = DXGI_SCALING_STRETCH;
        desc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_DISCARD;
        desc.AlphaMode = DXGI_ALPHA_MODE_IGNORE;
        desc.Flags = m_vsync ? 0 : DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING;

        ComPtr<IDXGISwapChain1> swapchain1;
        HRESULT hr = m_factory->CreateSwapChainForHwnd(
            m_queue.Get(), hwnd, &desc, nullptr, nullptr, &swapchain1);
        if (FAILED(hr)) {
            check(hr, "CreateSwapChainForHwnd");
            return false;
        }
        m_factory->MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER);
        hr = swapchain1.As(&m_swapchain);
        if (FAILED(hr)) return false;

        m_frameIndex = m_swapchain->GetCurrentBackBufferIndex();
        return createRenderTargets();
    }

    bool createRenderTargets() {
        for (u32 i = 0; i < m_renderTargets.size(); ++i) {
            m_renderTargets[i].Reset();
            HRESULT hr = m_swapchain->GetBuffer(i, IID_PPV_ARGS(&m_renderTargets[i]));
            if (FAILED(hr)) {
                check(hr, "GetBuffer");
                return false;
            }
            m_device->CreateRenderTargetView(m_renderTargets[i].Get(), nullptr, m_rtvHandles[i]);
        }
        return true;
    }

    bool createDescriptorHeaps() {
        D3D12_DESCRIPTOR_HEAP_DESC rtvDesc{};
        rtvDesc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
        rtvDesc.NumDescriptors = static_cast<UINT>(m_renderTargets.size());
        if (FAILED(m_device->CreateDescriptorHeap(&rtvDesc, IID_PPV_ARGS(&m_rtvHeap)))) {
            log::error("DX12 CreateDescriptorHeap(RTV) failed");
            return false;
        }
        const UINT rtvSize = m_device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
        D3D12_CPU_DESCRIPTOR_HANDLE rtv = m_rtvHeap->GetCPUDescriptorHandleForHeapStart();
        for (u32 i = 0; i < m_renderTargets.size(); ++i) {
            m_rtvHandles[i] = rtv;
            rtv.ptr += rtvSize;
        }

        D3D12_DESCRIPTOR_HEAP_DESC dsvDesc{};
        dsvDesc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_DSV;
        dsvDesc.NumDescriptors = 1;
        if (FAILED(m_device->CreateDescriptorHeap(&dsvDesc, IID_PPV_ARGS(&m_dsvHeap)))) {
            log::error("DX12 CreateDescriptorHeap(DSV) failed");
            return false;
        }
        return true;
    }

    bool createDepthBuffer() {
        D3D12_RESOURCE_DESC desc{};
        desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
        desc.Width = m_width;
        desc.Height = m_height;
        desc.DepthOrArraySize = 1;
        desc.MipLevels = 1;
        desc.Format = kDepthFormat;
        desc.SampleDesc.Count = 1;
        desc.Flags = D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL;

        D3D12_CLEAR_VALUE clear{};
        clear.Format = kDepthFormat;
        clear.DepthStencil.Depth = 1.0f;

        D3D12_HEAP_PROPERTIES heap{};
        heap.Type = D3D12_HEAP_TYPE_DEFAULT;
        HRESULT hr = m_device->CreateCommittedResource(
            &heap, D3D12_HEAP_FLAG_NONE, &desc,
            D3D12_RESOURCE_STATE_DEPTH_WRITE, &clear,
            IID_PPV_ARGS(&m_depth));
        if (FAILED(hr)) {
            check(hr, "CreateCommittedResource(depth)");
            return false;
        }

        D3D12_DEPTH_STENCIL_VIEW_DESC dsv{};
        dsv.Format = kDepthFormat;
        dsv.ViewDimension = D3D12_DSV_DIMENSION_TEXTURE2D;
        m_device->CreateDepthStencilView(
            m_depth.Get(), &dsv, m_dsvHeap->GetCPUDescriptorHandleForHeapStart());
        return true;
    }

    void waitForFrame() {
        const u64 value = m_frameFenceValues[m_frameSlot];
        if (value != 0 && m_fence->GetCompletedValue() < value) {
            m_fence->SetEventOnCompletion(value, m_fenceEvent);
            WaitForSingleObject(m_fenceEvent, INFINITE);
        }
    }

    ComPtr<IDXGIFactory6> m_factory;
    ComPtr<IDXGIAdapter1> m_adapter;
    ComPtr<ID3D12Device> m_device;
    ComPtr<ID3D12CommandQueue> m_queue;
    ComPtr<IDXGISwapChain3> m_swapchain;
    ComPtr<ID3D12DescriptorHeap> m_rtvHeap;
    ComPtr<ID3D12DescriptorHeap> m_dsvHeap;
    std::array<ComPtr<ID3D12Resource>, kFramesInFlight> m_renderTargets;
    std::array<D3D12_CPU_DESCRIPTOR_HANDLE, kFramesInFlight> m_rtvHandles{};
    ComPtr<ID3D12Resource> m_depth;
    std::array<ComPtr<ID3D12CommandAllocator>, kFramesInFlight> m_allocators;
    ComPtr<ID3D12GraphicsCommandList> m_commandList;
    ComPtr<ID3D12Fence> m_fence;
    std::array<u64, kFramesInFlight> m_frameFenceValues{};
    u64 m_fenceValue = 0;
    HANDLE m_fenceEvent = nullptr;
    u32 m_frameIndex = 0;
    u32 m_frameSlot = 0;
    u32 m_width = 0;
    u32 m_height = 0;
    bool m_vsync = true;
    bool m_debug = false;
    std::string m_adapterName;
    Stats m_stats;
};

} // namespace

std::unique_ptr<Device> createDX12Device() {
    return std::make_unique<DX12Device>();
}

} // namespace vd::rhi
