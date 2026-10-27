#include "void/rhi/RHI.h"

#include "void/core/Log.h"
#include "void/core/Paths.h"

#include <SDL3/SDL.h>
#include <d3d12.h>
#include <dxgi1_6.h>
#include <wrl/client.h>

#include <array>
#include <cstring>
#include <fstream>
#include <string>
#include <vector>

using Microsoft::WRL::ComPtr;

namespace vd::rhi {
namespace {

constexpr u32 kFramesInFlight = 2;
constexpr DXGI_FORMAT kSwapchainFormat = DXGI_FORMAT_B8G8R8A8_UNORM;
constexpr DXGI_FORMAT kDepthFormat = DXGI_FORMAT_D32_FLOAT;
constexpr u32 kShadowSize = 2048;
constexpr DXGI_FORMAT kShadowFormat = DXGI_FORMAT_R32_FLOAT;

struct PushData {
    glm::mat4 model{1.0f};
    glm::vec4 baseColor{1.0f};
    glm::vec4 material{0.6f, 0.0f, 0.0f, 0.0f};
    glm::vec4 uvScale{1.0f};
};

struct UIPush {
    glm::mat4 ortho{1.0f};
    glm::vec4 params{0.0f};
};

struct ShadowPush {
    glm::mat4 viewProj{1.0f};
    glm::mat4 model{1.0f};
};

struct MeshGPU {
    ComPtr<ID3D12Resource> vertexBuffer;
    ComPtr<ID3D12Resource> indexBuffer;
    D3D12_VERTEX_BUFFER_VIEW vbView{};
    D3D12_INDEX_BUFFER_VIEW ibView{};
    u32 indexCount = 0;
};

struct TextureGPU {
    ComPtr<ID3D12Resource> resource;
    u32 descriptorIndex = 0;
};

std::vector<u8> readFile(const std::filesystem::path& path) {
    std::ifstream file(path, std::ios::binary | std::ios::ate);
    if (!file) {
        log::error("DX12: cannot open shader %s", path.string().c_str());
        return {};
    }
    const std::streamsize size = file.tellg();
    file.seekg(0);
    std::vector<u8> data(static_cast<size_t>(size));
    file.read(reinterpret_cast<char*>(data.data()), size);
    return data;
}

void check(HRESULT hr, const char* what) {
    if (FAILED(hr)) {
        log::error("DX12 %s failed: 0x%08x", what, static_cast<unsigned>(hr));
    }
}

u32 alignTo(u32 value, u32 alignment) {
    return (value + alignment - 1) & ~(alignment - 1);
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
        log::info("DX12: selected adapter %s", m_adapterName.c_str());

        check(D3D12CreateDevice(m_adapter.Get(), D3D_FEATURE_LEVEL_12_0, IID_PPV_ARGS(&m_device)), "D3D12CreateDevice");
        if (!m_device) return false;
        if (m_debug) m_device.As(&m_infoQueue);

        D3D12_COMMAND_QUEUE_DESC queueDesc{};
        queueDesc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
        check(m_device->CreateCommandQueue(&queueDesc, IID_PPV_ARGS(&m_queue)), "CreateCommandQueue");
        if (!m_queue) return false;

        // Create these before any resources or swapchain: an early device loss on
        // some Intel drivers otherwise surfaces later as a failed PSO/root signature.
        log::debug("DX12: creating root signatures");
        if (!createRootSignatures()) return false;

        log::debug("DX12: creating command objects");
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
        if (!m_fenceEvent) return false;

        log::debug("DX12: creating descriptor heaps");
        if (!createDescriptorHeaps()) return false;
        log::debug("DX12: creating swapchain");
        if (!createSwapchain(desc.window)) return false;

        log::debug("DX12: creating frame constants");
        for (u32 i = 0; i < kFramesInFlight; ++i) {
            D3D12_HEAP_PROPERTIES heap{};
            heap.Type = D3D12_HEAP_TYPE_UPLOAD;
            D3D12_RESOURCE_DESC descCB{};
            descCB.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
            descCB.Width = alignTo(sizeof(FrameData), 256);
            descCB.Height = 1;
            descCB.DepthOrArraySize = 1;
            descCB.MipLevels = 1;
            descCB.Format = DXGI_FORMAT_UNKNOWN;
            descCB.SampleDesc.Count = 1;
            descCB.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
            check(m_device->CreateCommittedResource(
                &heap, D3D12_HEAP_FLAG_NONE, &descCB, D3D12_RESOURCE_STATE_GENERIC_READ, nullptr,
                IID_PPV_ARGS(&m_frameCB[i])), "CreateCommittedResource(frameCB)");
            m_frameCB[i]->Map(0, nullptr, reinterpret_cast<void**>(&m_frameCBMapped[i]));
        }

        log::debug("DX12: creating depth");
        if (!createDepthBuffer()) return false;
        log::debug("DX12: creating shadow");
        if (!createShadowMap()) return false;

        log::debug("DX12: creating pipelines");
        if (!createPipelines()) return false;
        log::debug("DX12: creating UI buffer");
        if (!createUIBuffer()) return false;

        log::info("DX12 adapter: %s", m_adapterName.c_str());
        log::info("DX12 validation: %s", m_debug ? "on" : "off");
        return true;
    }

    void shutdown() override {
        if (!m_device) return;
        waitIdle();
        m_textureArray = {};
        m_fontTexture = {};
        if (m_uiMapped) {
            m_uiBuffer->Unmap(0, nullptr);
            m_uiMapped = nullptr;
        }
        for (u32 i = 0; i < kFramesInFlight; ++i) {
            if (m_frameCBMapped[i]) {
                m_frameCB[i]->Unmap(0, nullptr);
                m_frameCBMapped[i] = nullptr;
            }
            m_frameCB[i].Reset();
            m_allocators[i].Reset();
        }
        m_uiBuffer.Reset();
        m_shadowColor.Reset();
        m_shadow.Reset();
        m_depth.Reset();
        m_dsvHeap.Reset();
        for (auto& target : m_renderTargets) target.Reset();
        m_rtvHeap.Reset();
        m_srvHeap.Reset();
        m_scenePSO.Reset();
        m_shadowPSO.Reset();
        m_skyPSO.Reset();
        m_uiPSO.Reset();
        m_sceneRoot.Reset();
        m_shadowRoot.Reset();
        m_uiRoot.Reset();
        m_commandList.Reset();
        if (m_fenceEvent) {
            CloseHandle(m_fenceEvent);
            m_fenceEvent = nullptr;
        }
        m_fence.Reset();
        m_swapchain.Reset();
        m_queue.Reset();
        m_device.Reset();
        m_adapter.Reset();
        m_factory.Reset();
        m_meshes.clear();
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

    MeshHandle createMesh(const MeshData& mesh) override {
        if (mesh.vertices.empty() || mesh.indices.empty()) return {};
        const u64 vbSize = static_cast<u64>(mesh.vertices.size() * sizeof(Vertex));
        const u64 ibSize = static_cast<u64>(mesh.indices.size() * sizeof(u32));

        MeshGPU gpu;
        if (!createUploadBuffer(vbSize, D3D12_RESOURCE_STATE_GENERIC_READ, gpu.vertexBuffer)) return {};
        if (!createUploadBuffer(ibSize, D3D12_RESOURCE_STATE_GENERIC_READ, gpu.indexBuffer)) return {};

        void* mapped = nullptr;
        gpu.vertexBuffer->Map(0, nullptr, &mapped);
        std::memcpy(mapped, mesh.vertices.data(), vbSize);
        gpu.vertexBuffer->Unmap(0, nullptr);
        gpu.indexBuffer->Map(0, nullptr, &mapped);
        std::memcpy(mapped, mesh.indices.data(), ibSize);
        gpu.indexBuffer->Unmap(0, nullptr);

        gpu.vbView.BufferLocation = gpu.vertexBuffer->GetGPUVirtualAddress();
        gpu.vbView.SizeInBytes = static_cast<UINT>(vbSize);
        gpu.vbView.StrideInBytes = sizeof(Vertex);
        gpu.ibView.BufferLocation = gpu.indexBuffer->GetGPUVirtualAddress();
        gpu.ibView.SizeInBytes = static_cast<UINT>(ibSize);
        gpu.ibView.Format = DXGI_FORMAT_R32_UINT;
        gpu.indexCount = static_cast<u32>(mesh.indices.size());

        m_meshes.push_back(std::move(gpu));
        return MeshHandle{static_cast<u32>(m_meshes.size())};
    }

    bool createTextureArray(const std::vector<ImageData>& layers) override {
        if (layers.empty()) return false;
        const u32 width = layers[0].width;
        const u32 height = layers[0].height;
        for (const auto& layer : layers) {
            if (layer.width != width || layer.height != height) return false;
        }

        D3D12_RESOURCE_DESC desc{};
        desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
        desc.Width = width;
        desc.Height = height;
        desc.DepthOrArraySize = static_cast<UINT16>(layers.size());
        desc.MipLevels = 1;
        desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM_SRGB;
        desc.SampleDesc.Count = 1;
        desc.Layout = D3D12_TEXTURE_LAYOUT_UNKNOWN;
        desc.Flags = D3D12_RESOURCE_FLAG_NONE;

        D3D12_HEAP_PROPERTIES heap{};
        heap.Type = D3D12_HEAP_TYPE_DEFAULT;
        m_textureArray.resource.Reset();
        HRESULT hr = m_device->CreateCommittedResource(
            &heap, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COPY_DEST, nullptr,
            IID_PPV_ARGS(&m_textureArray.resource));
        if (FAILED(hr)) {
            check(hr, "CreateTextureArray");
            return false;
        }

        const u32 rowPitch = alignTo(width * 4, D3D12_TEXTURE_DATA_PITCH_ALIGNMENT);
        const u32 slicePitch = rowPitch * height;
        const u64 uploadSize = static_cast<u64>(slicePitch) * layers.size();
        ComPtr<ID3D12Resource> upload;
        if (!createUploadBuffer(uploadSize, D3D12_RESOURCE_STATE_GENERIC_READ, upload)) return false;

        u8* mapped = nullptr;
        upload->Map(0, nullptr, reinterpret_cast<void**>(&mapped));
        for (u32 layer = 0; layer < layers.size(); ++layer) {
            for (u32 y = 0; y < height; ++y) {
                std::memcpy(mapped + static_cast<u64>(layer) * slicePitch + y * rowPitch,
                            layers[layer].pixels.data() + static_cast<u64>(y) * width * 4,
                            width * 4);
            }
        }
        upload->Unmap(0, nullptr);

        auto cmd = beginImmediate();
        if (!cmd) return false;
        for (u32 layer = 0; layer < layers.size(); ++layer) {
            D3D12_TEXTURE_COPY_LOCATION dst{};
            dst.pResource = m_textureArray.resource.Get();
            dst.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
            dst.SubresourceIndex = layer;
            D3D12_TEXTURE_COPY_LOCATION src{};
            src.pResource = upload.Get();
            src.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
            src.PlacedFootprint.Footprint.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
            src.PlacedFootprint.Footprint.Width = width;
            src.PlacedFootprint.Footprint.Height = height;
            src.PlacedFootprint.Footprint.Depth = 1;
            src.PlacedFootprint.Footprint.RowPitch = rowPitch;
            src.PlacedFootprint.Offset = static_cast<UINT64>(layer) * slicePitch;
            cmd->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
        }
        transitionTexture(cmd.Get(), m_textureArray.resource.Get(), D3D12_RESOURCE_STATE_COPY_DEST,
                          D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, static_cast<u32>(layers.size()));
        endImmediate(cmd);

        D3D12_SHADER_RESOURCE_VIEW_DESC srv{};
        srv.Format = DXGI_FORMAT_R8G8B8A8_UNORM_SRGB;
        srv.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2DARRAY;
        srv.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
        srv.Texture2DArray.MipLevels = 1;
        srv.Texture2DArray.ArraySize = static_cast<UINT>(layers.size());
        srv.Texture2DArray.FirstArraySlice = 0;
        srv.Texture2DArray.MostDetailedMip = 0;
        m_textureArray.descriptorIndex = 1;
        m_device->CreateShaderResourceView(m_textureArray.resource.Get(), &srv,
                                           descriptorHandle(m_textureArray.descriptorIndex));
        return true;
    }

    bool createFontTexture(const ImageData& image) override {
        if (image.pixels.empty() || image.width == 0 || image.height == 0) return false;
        D3D12_RESOURCE_DESC desc{};
        desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
        desc.Width = image.width;
        desc.Height = image.height;
        desc.DepthOrArraySize = 1;
        desc.MipLevels = 1;
        desc.Format = DXGI_FORMAT_R8_UNORM;
        desc.SampleDesc.Count = 1;
        desc.Layout = D3D12_TEXTURE_LAYOUT_UNKNOWN;

        D3D12_HEAP_PROPERTIES heap{};
        heap.Type = D3D12_HEAP_TYPE_DEFAULT;
        m_fontTexture.resource.Reset();
        HRESULT hr = m_device->CreateCommittedResource(
            &heap, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COPY_DEST, nullptr,
            IID_PPV_ARGS(&m_fontTexture.resource));
        if (FAILED(hr)) return false;

        const u32 rowPitch = alignTo(image.width, D3D12_TEXTURE_DATA_PITCH_ALIGNMENT);
        ComPtr<ID3D12Resource> upload;
        if (!createUploadBuffer(static_cast<u64>(rowPitch) * image.height, D3D12_RESOURCE_STATE_GENERIC_READ, upload)) {
            return false;
        }
        u8* mapped = nullptr;
        upload->Map(0, nullptr, reinterpret_cast<void**>(&mapped));
        for (u32 y = 0; y < image.height; ++y) {
            std::memcpy(mapped + static_cast<u64>(y) * rowPitch,
                        image.pixels.data() + static_cast<u64>(y) * image.width,
                        image.width);
        }
        upload->Unmap(0, nullptr);

        auto cmd = beginImmediate();
        if (!cmd) return false;
        D3D12_TEXTURE_COPY_LOCATION dst{};
        dst.pResource = m_fontTexture.resource.Get();
        dst.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
        dst.SubresourceIndex = 0;
        D3D12_TEXTURE_COPY_LOCATION src{};
        src.pResource = upload.Get();
        src.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
        src.PlacedFootprint.Footprint.Format = DXGI_FORMAT_R8_UNORM;
        src.PlacedFootprint.Footprint.Width = image.width;
        src.PlacedFootprint.Footprint.Height = image.height;
        src.PlacedFootprint.Footprint.Depth = 1;
        src.PlacedFootprint.Footprint.RowPitch = rowPitch;
        cmd->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
        transitionTexture(cmd.Get(), m_fontTexture.resource.Get(), D3D12_RESOURCE_STATE_COPY_DEST,
                          D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, 1);
        endImmediate(cmd);

        D3D12_SHADER_RESOURCE_VIEW_DESC srv{};
        srv.Format = DXGI_FORMAT_R8_UNORM;
        srv.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
        srv.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
        srv.Texture2D.MipLevels = 1;
        m_fontTexture.descriptorIndex = 2;
        m_device->CreateShaderResourceView(m_fontTexture.resource.Get(), &srv,
                                           descriptorHandle(m_fontTexture.descriptorIndex));
        return true;
    }

    void render(const FrameData& frame, const DrawList& draws, const UIList& ui) override {
        if (!m_swapchain || !m_scenePSO) return;
        waitForFrame();
        m_stats = {};

        const u32 index = m_swapchain->GetCurrentBackBufferIndex();
        m_allocators[m_frameSlot]->Reset();
        m_commandList->Reset(m_allocators[m_frameSlot].Get(), nullptr);

        std::memcpy(m_frameCBMapped[m_frameSlot], &frame, sizeof(FrameData));
        if (!ui.vertices.empty()) {
            const u64 bytes = static_cast<u64>(ui.vertices.size() * sizeof(UIVertex));
            if (bytes <= m_uiCapacity) {
                std::memcpy(m_uiMapped, ui.vertices.data(), bytes);
            }
        }

        drawShadowPass(draws);
        drawMainPass(draws, ui, index);

        m_commandList->Close();
        ID3D12CommandList* lists[] = {m_commandList.Get()};
        m_queue->ExecuteCommandLists(1, lists);

        const UINT syncInterval = m_vsync ? 1 : 0;
        const UINT presentFlags = m_vsync ? 0 : DXGI_PRESENT_ALLOW_TEARING;
        HRESULT presentHr = m_swapchain->Present(syncInterval, presentFlags);
        if (FAILED(presentHr)) log::error("DX12 Present failed: 0x%08x", static_cast<unsigned>(presentHr));

        const u64 value = ++m_fenceValue;
        m_queue->Signal(m_fence.Get(), value);
        m_frameFenceValues[m_frameSlot] = value;
        m_frameSlot = (m_frameSlot + 1) % kFramesInFlight;
        if (m_frameCount < 3) {
            ++m_frameCount;
            logMessages("frame");
        }
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

    D3D12_CPU_DESCRIPTOR_HANDLE descriptorHandle(u32 index) const {
        D3D12_CPU_DESCRIPTOR_HANDLE handle = m_srvHeap->GetCPUDescriptorHandleForHeapStart();
        handle.ptr += static_cast<SIZE_T>(index) * m_srvDescriptorSize;
        return handle;
    }

    D3D12_GPU_DESCRIPTOR_HANDLE descriptorHandleGPU(u32 index) const {
        D3D12_GPU_DESCRIPTOR_HANDLE handle = m_srvHeap->GetGPUDescriptorHandleForHeapStart();
        handle.ptr += static_cast<UINT64>(index) * m_srvDescriptorSize;
        return handle;
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
            if (FAILED(hr)) return false;
            m_device->CreateRenderTargetView(m_renderTargets[i].Get(), nullptr, m_rtvHandles[i]);
        }
        return true;
    }

    bool createDescriptorHeaps() {
        D3D12_DESCRIPTOR_HEAP_DESC rtvDesc{};
        rtvDesc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
        rtvDesc.NumDescriptors = static_cast<UINT>(m_renderTargets.size()) + 1;
        if (FAILED(m_device->CreateDescriptorHeap(&rtvDesc, IID_PPV_ARGS(&m_rtvHeap)))) return false;
        const UINT rtvSize = m_device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
        D3D12_CPU_DESCRIPTOR_HANDLE rtv = m_rtvHeap->GetCPUDescriptorHandleForHeapStart();
        for (u32 i = 0; i < m_renderTargets.size(); ++i) {
            m_rtvHandles[i] = rtv;
            rtv.ptr += rtvSize;
        }
        m_rtvShadow = rtv;

        D3D12_DESCRIPTOR_HEAP_DESC dsvDesc{};
        dsvDesc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_DSV;
        dsvDesc.NumDescriptors = 2;
        if (FAILED(m_device->CreateDescriptorHeap(&dsvDesc, IID_PPV_ARGS(&m_dsvHeap)))) return false;
        const UINT dsvSize = m_device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_DSV);
        m_dsvMain = m_dsvHeap->GetCPUDescriptorHandleForHeapStart();
        m_dsvShadow = m_dsvMain;
        m_dsvShadow.ptr += dsvSize;

        D3D12_DESCRIPTOR_HEAP_DESC srvDesc{};
        srvDesc.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
        srvDesc.NumDescriptors = 4;
        srvDesc.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
        if (FAILED(m_device->CreateDescriptorHeap(&srvDesc, IID_PPV_ARGS(&m_srvHeap)))) return false;
        m_srvDescriptorSize = m_device->GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
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
        if (FAILED(hr)) return false;

        D3D12_DEPTH_STENCIL_VIEW_DESC dsv{};
        dsv.Format = kDepthFormat;
        dsv.ViewDimension = D3D12_DSV_DIMENSION_TEXTURE2D;
        m_device->CreateDepthStencilView(m_depth.Get(), &dsv, m_dsvMain);
        return true;
    }

    bool createShadowMap() {
        D3D12_RESOURCE_DESC depthDesc{};
        depthDesc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
        depthDesc.Width = kShadowSize;
        depthDesc.Height = kShadowSize;
        depthDesc.DepthOrArraySize = 1;
        depthDesc.MipLevels = 1;
        depthDesc.Format = kDepthFormat;
        depthDesc.SampleDesc.Count = 1;
        depthDesc.Flags = D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL;
        D3D12_CLEAR_VALUE clear{};
        clear.Format = kDepthFormat;
        clear.DepthStencil.Depth = 1.0f;
        D3D12_HEAP_PROPERTIES heap{};
        heap.Type = D3D12_HEAP_TYPE_DEFAULT;
        if (FAILED(m_device->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &depthDesc,
                D3D12_RESOURCE_STATE_DEPTH_WRITE, &clear, IID_PPV_ARGS(&m_shadow)))) return false;
        D3D12_DEPTH_STENCIL_VIEW_DESC dsv{};
        dsv.Format = kDepthFormat;
        dsv.ViewDimension = D3D12_DSV_DIMENSION_TEXTURE2D;
        m_device->CreateDepthStencilView(m_shadow.Get(), &dsv, m_dsvShadow);

        D3D12_RESOURCE_DESC colorDesc{};
        colorDesc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
        colorDesc.Width = kShadowSize;
        colorDesc.Height = kShadowSize;
        colorDesc.DepthOrArraySize = 1;
        colorDesc.MipLevels = 1;
        colorDesc.Format = kShadowFormat;
        colorDesc.SampleDesc.Count = 1;
        colorDesc.Flags = D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET;
        D3D12_CLEAR_VALUE colorClear{};
        colorClear.Format = kShadowFormat;
        colorClear.Color[0] = 1.0f;
        colorClear.Color[1] = 1.0f;
        colorClear.Color[2] = 1.0f;
        colorClear.Color[3] = 1.0f;
        if (FAILED(m_device->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &colorDesc,
                D3D12_RESOURCE_STATE_RENDER_TARGET, &colorClear, IID_PPV_ARGS(&m_shadowColor)))) return false;
        m_device->CreateRenderTargetView(m_shadowColor.Get(), nullptr, m_rtvShadow);
        D3D12_SHADER_RESOURCE_VIEW_DESC srv{};
        srv.Format = kShadowFormat;
        srv.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
        srv.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
        srv.Texture2D.MipLevels = 1;
        m_device->CreateShaderResourceView(m_shadowColor.Get(), &srv, descriptorHandle(0));
        return true;
    }

    bool createUploadBuffer(u64 size, D3D12_RESOURCE_STATES initial, ComPtr<ID3D12Resource>& out) {
        D3D12_HEAP_PROPERTIES heap{};
        heap.Type = D3D12_HEAP_TYPE_UPLOAD;
        D3D12_RESOURCE_DESC desc{};
        desc.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
        desc.Width = size;
        desc.Height = 1;
        desc.DepthOrArraySize = 1;
        desc.MipLevels = 1;
        desc.Format = DXGI_FORMAT_UNKNOWN;
        desc.SampleDesc.Count = 1;
        desc.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
        HRESULT hr = m_device->CreateCommittedResource(
            &heap, D3D12_HEAP_FLAG_NONE, &desc, initial, nullptr, IID_PPV_ARGS(&out));
        return SUCCEEDED(hr);
    }

    ComPtr<ID3D12GraphicsCommandList> beginImmediate() {
        ComPtr<ID3D12CommandAllocator> allocator;
        ComPtr<ID3D12GraphicsCommandList> list;
        if (FAILED(m_device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&allocator)))) return {};
        if (FAILED(m_device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, allocator.Get(), nullptr,
                                               IID_PPV_ARGS(&list)))) return {};
        m_immediateAllocator = allocator;
        m_immediateList = list;
        return list;
    }

    void transitionTexture(ID3D12GraphicsCommandList* cmd, ID3D12Resource* resource,
                           D3D12_RESOURCE_STATES before, D3D12_RESOURCE_STATES after, u32 subresources) {
        D3D12_RESOURCE_BARRIER barrier{};
        barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        barrier.Transition.pResource = resource;
        barrier.Transition.Subresource = subresources == 1 ? 0 : D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
        barrier.Transition.StateBefore = before;
        barrier.Transition.StateAfter = after;
        cmd->ResourceBarrier(1, &barrier);
    }

    void endImmediate(const ComPtr<ID3D12GraphicsCommandList>& list) {
        list->Close();
        ID3D12CommandList* lists[] = {list.Get()};
        m_queue->ExecuteCommandLists(1, lists);
        waitIdle();
        m_immediateList.Reset();
        m_immediateAllocator.Reset();
    }

    bool createRootSignatures() {
        D3D12_STATIC_SAMPLER_DESC samplers[2]{};
        samplers[0].Filter = D3D12_FILTER_MIN_MAG_MIP_LINEAR;
        samplers[0].AddressU = D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
        samplers[0].AddressV = D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
        samplers[0].AddressW = D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
        samplers[0].ComparisonFunc = D3D12_COMPARISON_FUNC_ALWAYS;
        samplers[0].MaxAnisotropy = 1;
        samplers[0].ShaderRegister = 0;
        samplers[0].ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
        samplers[0].BorderColor = D3D12_STATIC_BORDER_COLOR_OPAQUE_WHITE;

        samplers[1].Filter = D3D12_FILTER_MIN_MAG_MIP_LINEAR;
        samplers[1].AddressU = D3D12_TEXTURE_ADDRESS_MODE_WRAP;
        samplers[1].AddressV = D3D12_TEXTURE_ADDRESS_MODE_WRAP;
        samplers[1].AddressW = D3D12_TEXTURE_ADDRESS_MODE_WRAP;
        samplers[1].MaxAnisotropy = 8;
        samplers[1].ShaderRegister = 1;
        samplers[1].ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;

        D3D12_DESCRIPTOR_RANGE ranges[2]{};
        ranges[0].RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_SRV;
        ranges[0].NumDescriptors = 1;
        ranges[0].BaseShaderRegister = 0;
        ranges[0].OffsetInDescriptorsFromTableStart = 0;
        ranges[1].RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_SRV;
        ranges[1].NumDescriptors = 1;
        ranges[1].BaseShaderRegister = 1;
        ranges[1].OffsetInDescriptorsFromTableStart = 1;

        D3D12_ROOT_PARAMETER params[3]{};
        params[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_CBV;
        params[0].Descriptor.ShaderRegister = 0;
        params[0].ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
        params[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
        params[1].DescriptorTable.NumDescriptorRanges = 2;
        params[1].DescriptorTable.pDescriptorRanges = ranges;
        params[1].ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
        params[2].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
        params[2].Constants.ShaderRegister = 2;
        params[2].Constants.Num32BitValues = 28;
        params[2].ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;

        D3D12_ROOT_SIGNATURE_DESC desc{};
        desc.NumParameters = 3;
        desc.pParameters = params;
        desc.NumStaticSamplers = 2;
        desc.pStaticSamplers = samplers;
        desc.Flags = D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT;
        if (!serializeRoot(desc, m_sceneRoot)) {
            log::error("DX12 scene root signature failed");
            return false;
        }

        D3D12_ROOT_PARAMETER shadowParam{};
        shadowParam.ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
        shadowParam.Constants.ShaderRegister = 0;
        shadowParam.Constants.Num32BitValues = 32;
        shadowParam.ShaderVisibility = D3D12_SHADER_VISIBILITY_VERTEX;
        D3D12_ROOT_SIGNATURE_DESC shadowDesc{};
        shadowDesc.NumParameters = 1;
        shadowDesc.pParameters = &shadowParam;
        shadowDesc.Flags = D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT;
        if (!serializeRoot(shadowDesc, m_shadowRoot)) {
            log::error("DX12 shadow root signature failed");
            return false;
        }

        D3D12_DESCRIPTOR_RANGE uiRange{};
        uiRange.RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_SRV;
        uiRange.NumDescriptors = 1;
        uiRange.BaseShaderRegister = 0;
        D3D12_ROOT_PARAMETER uiParams[2]{};
        uiParams[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
        uiParams[0].Constants.ShaderRegister = 0;
        uiParams[0].Constants.Num32BitValues = 20;
        uiParams[0].ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
        uiParams[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
        uiParams[1].DescriptorTable.NumDescriptorRanges = 1;
        uiParams[1].DescriptorTable.pDescriptorRanges = &uiRange;
        uiParams[1].ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
        D3D12_STATIC_SAMPLER_DESC uiSampler{};
        uiSampler.Filter = D3D12_FILTER_MIN_MAG_MIP_LINEAR;
        uiSampler.AddressU = D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
        uiSampler.AddressV = D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
        uiSampler.AddressW = D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
        uiSampler.ShaderRegister = 0;
        uiSampler.ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
        D3D12_ROOT_SIGNATURE_DESC uiDesc{};
        uiDesc.NumParameters = 2;
        uiDesc.pParameters = uiParams;
        uiDesc.NumStaticSamplers = 1;
        uiDesc.pStaticSamplers = &uiSampler;
        uiDesc.Flags = D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT;
        if (!serializeRoot(uiDesc, m_uiRoot)) {
            log::error("DX12 UI root signature failed");
            return false;
        }
        return true;
    }

    bool serializeRoot(const D3D12_ROOT_SIGNATURE_DESC& desc, ComPtr<ID3D12RootSignature>& out) {
        ComPtr<ID3DBlob> blob;
        ComPtr<ID3DBlob> error;
        HRESULT hr = D3D12SerializeRootSignature(&desc, D3D_ROOT_SIGNATURE_VERSION_1, &blob, &error);
        if (FAILED(hr)) {
            if (error) log::error("DX12 root signature: %s", static_cast<const char*>(error->GetBufferPointer()));
            return false;
        }
        HRESULT createHr = m_device->CreateRootSignature(0, blob->GetBufferPointer(), blob->GetBufferSize(),
                                                         IID_PPV_ARGS(&out));
        if (FAILED(createHr)) {
            log::error("DX12 CreateRootSignature failed: 0x%08x", static_cast<unsigned>(createHr));
            return false;
        }
        return true;
    }

    bool createPipelines() {
        auto sceneVS = readFile(Paths::asset("shaders/scene.vs.dxil"));
        auto scenePS = readFile(Paths::asset("shaders/scene.ps.dxil"));
        auto shadowVS = readFile(Paths::asset("shaders/shadow.vs.dxil"));
        auto shadowPS = readFile(Paths::asset("shaders/shadow.ps.dxil"));
        auto skyVS = readFile(Paths::asset("shaders/sky.vs.dxil"));
        auto skyPS = readFile(Paths::asset("shaders/sky.ps.dxil"));
        auto uiVS = readFile(Paths::asset("shaders/ui.vs.dxil"));
        auto uiPS = readFile(Paths::asset("shaders/ui.ps.dxil"));
        if (sceneVS.empty() || scenePS.empty() || shadowVS.empty() || shadowPS.empty() || skyVS.empty() || skyPS.empty() ||
            uiVS.empty() || uiPS.empty()) {
            return false;
        }

        D3D12_INPUT_ELEMENT_DESC inputLayout[] = {
            {"POSITION", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, 0, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
            {"NORMAL", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, 12, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
            {"TEXCOORD", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 24, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
        };
        D3D12_INPUT_LAYOUT_DESC inputDesc{inputLayout, 3};

        D3D12_RASTERIZER_DESC raster{};
        raster.FillMode = D3D12_FILL_MODE_SOLID;
        raster.CullMode = D3D12_CULL_MODE_NONE;
        raster.FrontCounterClockwise = TRUE;
        raster.DepthClipEnable = TRUE;

        D3D12_BLEND_DESC blend{};
        blend.RenderTarget[0].BlendEnable = FALSE;
        blend.RenderTarget[0].RenderTargetWriteMask = D3D12_COLOR_WRITE_ENABLE_ALL;

        D3D12_DEPTH_STENCIL_DESC depth{};
        depth.DepthEnable = TRUE;
        depth.DepthWriteMask = D3D12_DEPTH_WRITE_MASK_ALL;
        depth.DepthFunc = D3D12_COMPARISON_FUNC_LESS_EQUAL;
        depth.StencilEnable = FALSE;

        D3D12_GRAPHICS_PIPELINE_STATE_DESC pso{};
        pso.pRootSignature = m_sceneRoot.Get();
        pso.VS = {sceneVS.data(), sceneVS.size()};
        pso.PS = {scenePS.data(), scenePS.size()};
        pso.InputLayout = inputDesc;
        pso.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
        pso.RasterizerState = raster;
        pso.BlendState = blend;
        pso.DepthStencilState = depth;
        pso.DSVFormat = kDepthFormat;
        pso.NumRenderTargets = 1;
        pso.RTVFormats[0] = kSwapchainFormat;
        pso.SampleDesc.Count = 1;
        pso.SampleMask = UINT_MAX;
        if (FAILED(m_device->CreateGraphicsPipelineState(&pso, IID_PPV_ARGS(&m_scenePSO)))) {
            log::error("DX12 scene PSO failed");
            return false;
        }

        D3D12_GRAPHICS_PIPELINE_STATE_DESC shadowPso = pso;
        shadowPso.pRootSignature = m_shadowRoot.Get();
        shadowPso.VS = {shadowVS.data(), shadowVS.size()};
        shadowPso.PS = {shadowPS.data(), shadowPS.size()};
        shadowPso.NumRenderTargets = 1;
        shadowPso.RTVFormats[0] = kShadowFormat;
        shadowPso.DSVFormat = kDepthFormat;
        shadowPso.RasterizerState.DepthBias = 1000;
        shadowPso.RasterizerState.SlopeScaledDepthBias = 1.5f;
        shadowPso.RasterizerState.DepthBiasClamp = 0.0f;
        if (FAILED(m_device->CreateGraphicsPipelineState(&shadowPso, IID_PPV_ARGS(&m_shadowPSO)))) {
            log::error("DX12 shadow PSO failed");
            return false;
        }

        D3D12_GRAPHICS_PIPELINE_STATE_DESC skyPso = pso;
        skyPso.InputLayout = {};
        skyPso.VS = {skyVS.data(), skyVS.size()};
        skyPso.PS = {skyPS.data(), skyPS.size()};
        skyPso.DepthStencilState.DepthEnable = FALSE;
        skyPso.DepthStencilState.DepthWriteMask = D3D12_DEPTH_WRITE_MASK_ZERO;
        skyPso.RasterizerState.CullMode = D3D12_CULL_MODE_NONE;
        if (FAILED(m_device->CreateGraphicsPipelineState(&skyPso, IID_PPV_ARGS(&m_skyPSO)))) {
            log::error("DX12 sky PSO failed");
            return false;
        }

        D3D12_INPUT_ELEMENT_DESC uiLayout[] = {
            {"POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
            {"TEXCOORD", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 8, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
            {"COLOR", 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0, 16, D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
        };
        D3D12_GRAPHICS_PIPELINE_STATE_DESC uiPso{};
        uiPso.pRootSignature = m_uiRoot.Get();
        uiPso.VS = {uiVS.data(), uiVS.size()};
        uiPso.PS = {uiPS.data(), uiPS.size()};
        uiPso.InputLayout = {uiLayout, 3};
        uiPso.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
        uiPso.RasterizerState = raster;
        uiPso.RasterizerState.CullMode = D3D12_CULL_MODE_NONE;
        uiPso.DepthStencilState.DepthEnable = FALSE;
        uiPso.DepthStencilState.DepthWriteMask = D3D12_DEPTH_WRITE_MASK_ZERO;
        uiPso.BlendState = blend;
        uiPso.BlendState.RenderTarget[0].BlendEnable = TRUE;
        uiPso.BlendState.RenderTarget[0].SrcBlend = D3D12_BLEND_SRC_ALPHA;
        uiPso.BlendState.RenderTarget[0].DestBlend = D3D12_BLEND_INV_SRC_ALPHA;
        uiPso.BlendState.RenderTarget[0].BlendOp = D3D12_BLEND_OP_ADD;
        uiPso.BlendState.RenderTarget[0].SrcBlendAlpha = D3D12_BLEND_ONE;
        uiPso.BlendState.RenderTarget[0].DestBlendAlpha = D3D12_BLEND_INV_SRC_ALPHA;
        uiPso.BlendState.RenderTarget[0].BlendOpAlpha = D3D12_BLEND_OP_ADD;
        uiPso.DSVFormat = DXGI_FORMAT_UNKNOWN;
        uiPso.NumRenderTargets = 1;
        uiPso.RTVFormats[0] = kSwapchainFormat;
        uiPso.SampleDesc.Count = 1;
        uiPso.SampleMask = UINT_MAX;
        if (FAILED(m_device->CreateGraphicsPipelineState(&uiPso, IID_PPV_ARGS(&m_uiPSO)))) {
            log::error("DX12 UI PSO failed");
            return false;
        }
        return true;
    }

    bool createUIBuffer() {
        m_uiCapacity = 4 * 1024 * 1024;
        if (!createUploadBuffer(m_uiCapacity, D3D12_RESOURCE_STATE_GENERIC_READ, m_uiBuffer)) return false;
        m_uiBuffer->Map(0, nullptr, reinterpret_cast<void**>(&m_uiMapped));
        m_uiVB.BufferLocation = m_uiBuffer->GetGPUVirtualAddress();
        m_uiVB.SizeInBytes = static_cast<UINT>(m_uiCapacity);
        m_uiVB.StrideInBytes = sizeof(UIVertex);
        return true;
    }

    void drawShadowPass(const DrawList& draws) {
        if (m_shadowColorRead) {
            D3D12_RESOURCE_BARRIER barrier{};
            barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
            barrier.Transition.pResource = m_shadowColor.Get();
            barrier.Transition.Subresource = 0;
            barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
            barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_RENDER_TARGET;
            m_commandList->ResourceBarrier(1, &barrier);
        }
        m_shadowColorRead = false;

        m_commandList->OMSetRenderTargets(1, &m_rtvShadow, FALSE, &m_dsvShadow);
        const float shadowClear[4] = {1.0f, 1.0f, 1.0f, 1.0f};
        m_commandList->ClearRenderTargetView(m_rtvShadow, shadowClear, 0, nullptr);
        m_commandList->ClearDepthStencilView(m_dsvShadow, D3D12_CLEAR_FLAG_DEPTH, 1.0f, 0, 0, nullptr);
        D3D12_VIEWPORT viewport{0.0f, 0.0f, static_cast<float>(kShadowSize), static_cast<float>(kShadowSize), 0.0f, 1.0f};
        D3D12_RECT scissor{0, 0, kShadowSize, kShadowSize};
        m_commandList->RSSetViewports(1, &viewport);
        m_commandList->RSSetScissorRects(1, &scissor);
        m_commandList->SetPipelineState(m_shadowPSO.Get());
        m_commandList->SetGraphicsRootSignature(m_shadowRoot.Get());
        m_commandList->IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

        for (const auto& item : draws.items) {
            if (!item.castShadow) continue;
            if (item.mesh.id == 0 || item.mesh.id > m_meshes.size()) continue;
            const MeshGPU& mesh = m_meshes[item.mesh.id - 1];
            m_commandList->IASetVertexBuffers(0, 1, &mesh.vbView);
            m_commandList->IASetIndexBuffer(&mesh.ibView);
            ShadowPush push;
            push.viewProj = draws.shadowViewProj;
            push.model = item.transform;
            m_commandList->SetGraphicsRoot32BitConstants(0, 32, &push, 0);
            m_commandList->DrawIndexedInstanced(mesh.indexCount, 1, 0, 0, 0);
            ++m_stats.drawCalls;
        }

        D3D12_RESOURCE_BARRIER readBarrier{};
        readBarrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        readBarrier.Transition.pResource = m_shadowColor.Get();
        readBarrier.Transition.Subresource = 0;
        readBarrier.Transition.StateBefore = D3D12_RESOURCE_STATE_RENDER_TARGET;
        readBarrier.Transition.StateAfter = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
        m_commandList->ResourceBarrier(1, &readBarrier);
        m_shadowColorRead = true;
    }

    void drawMainPass(const DrawList& draws, const UIList& ui, u32 index) {
        D3D12_RESOURCE_BARRIER barrier{};
        barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        barrier.Transition.pResource = m_renderTargets[index].Get();
        barrier.Transition.Subresource = 0;
        barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_PRESENT;
        barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_RENDER_TARGET;
        m_commandList->ResourceBarrier(1, &barrier);

        const D3D12_CPU_DESCRIPTOR_HANDLE rtv = m_rtvHandles[index];
        m_commandList->OMSetRenderTargets(1, &rtv, FALSE, &m_dsvMain);
        const float clear[4] = {0.035f, 0.045f, 0.055f, 1.0f};
        m_commandList->ClearRenderTargetView(rtv, clear, 0, nullptr);
        m_commandList->ClearDepthStencilView(m_dsvMain, D3D12_CLEAR_FLAG_DEPTH, 1.0f, 0, 0, nullptr);

        D3D12_VIEWPORT viewport{0.0f, 0.0f, static_cast<float>(m_width), static_cast<float>(m_height), 0.0f, 1.0f};
        D3D12_RECT scissor{0, 0, static_cast<LONG>(m_width), static_cast<LONG>(m_height)};
        m_commandList->RSSetViewports(1, &viewport);
        m_commandList->RSSetScissorRects(1, &scissor);

        ID3D12DescriptorHeap* heaps[] = {m_srvHeap.Get()};
        m_commandList->SetDescriptorHeaps(1, heaps);

        m_commandList->SetGraphicsRootSignature(m_sceneRoot.Get());
        m_commandList->SetGraphicsRootConstantBufferView(0, m_frameCB[m_frameSlot]->GetGPUVirtualAddress());
        m_commandList->SetGraphicsRootDescriptorTable(1, descriptorHandleGPU(0));

        m_commandList->SetPipelineState(m_skyPSO.Get());
        m_commandList->IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        m_commandList->DrawInstanced(3, 1, 0, 0);
        ++m_stats.drawCalls;

        m_commandList->SetPipelineState(m_scenePSO.Get());
        for (const auto& item : draws.items) {
            if (item.mesh.id == 0 || item.mesh.id > m_meshes.size()) continue;
            const MeshGPU& mesh = m_meshes[item.mesh.id - 1];
            m_commandList->IASetVertexBuffers(0, 1, &mesh.vbView);
            m_commandList->IASetIndexBuffer(&mesh.ibView);
            PushData push;
            push.model = item.transform;
            push.baseColor = item.baseColor;
            push.material = {item.roughness, item.metallic, item.emissive, static_cast<float>(item.textureLayer)};
            push.uvScale = {item.uvScale.x, item.uvScale.y, 0.0f, 0.0f};
            m_commandList->SetGraphicsRoot32BitConstants(2, 28, &push, 0);
            m_commandList->DrawIndexedInstanced(mesh.indexCount, 1, 0, 0, 0);
            ++m_stats.drawCalls;
            m_stats.triangles += mesh.indexCount / 3;
        }

        if (!ui.vertices.empty()) {
            const u64 bytes = static_cast<u64>(ui.vertices.size() * sizeof(UIVertex));
            if (bytes <= m_uiCapacity) {
                m_commandList->SetGraphicsRootSignature(m_uiRoot.Get());
                m_commandList->SetPipelineState(m_uiPSO.Get());
                m_commandList->SetGraphicsRootDescriptorTable(1, descriptorHandleGPU(2));
                UIPush push;
                push.ortho = glm::ortho(0.0f, static_cast<float>(m_width), static_cast<float>(m_height), 0.0f, -1.0f, 1.0f);
                m_commandList->SetGraphicsRoot32BitConstants(0, 20, &push, 0);
                m_commandList->IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                m_commandList->IASetVertexBuffers(0, 1, &m_uiVB);
                m_commandList->DrawInstanced(static_cast<UINT>(ui.vertices.size()), 1, 0, 0);
                ++m_stats.drawCalls;
            }
        }

        D3D12_RESOURCE_BARRIER presentBarrier{};
        presentBarrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        presentBarrier.Transition.pResource = m_renderTargets[index].Get();
        presentBarrier.Transition.Subresource = 0;
        presentBarrier.Transition.StateBefore = D3D12_RESOURCE_STATE_RENDER_TARGET;
        presentBarrier.Transition.StateAfter = D3D12_RESOURCE_STATE_PRESENT;
        m_commandList->ResourceBarrier(1, &presentBarrier);
    }

    void logMessages(const char* context) {
        if (!m_infoQueue) return;
        const UINT64 count = m_infoQueue->GetNumStoredMessages();
        for (UINT64 i = 0; i < count; ++i) {
            SIZE_T size = 0;
            m_infoQueue->GetMessage(i, nullptr, &size);
            if (!size) continue;
            std::vector<char> buffer(size);
            auto* message = reinterpret_cast<D3D12_MESSAGE*>(buffer.data());
            if (SUCCEEDED(m_infoQueue->GetMessage(i, message, &size))) {
                if (message->Severity <= D3D12_MESSAGE_SEVERITY_WARNING) {
                    log::warn("DX12 %s: %s", context, message->pDescription);
                }
            }
        }
        m_infoQueue->ClearStoredMessages();
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
    ComPtr<ID3D12DescriptorHeap> m_srvHeap;
    std::array<ComPtr<ID3D12Resource>, kFramesInFlight> m_renderTargets;
    std::array<D3D12_CPU_DESCRIPTOR_HANDLE, kFramesInFlight> m_rtvHandles{};
    D3D12_CPU_DESCRIPTOR_HANDLE m_dsvMain{};
    D3D12_CPU_DESCRIPTOR_HANDLE m_dsvShadow{};
    D3D12_CPU_DESCRIPTOR_HANDLE m_rtvShadow{};
    ComPtr<ID3D12Resource> m_depth;
    ComPtr<ID3D12Resource> m_shadow;
    ComPtr<ID3D12Resource> m_shadowColor;
    bool m_shadowColorRead = false;
    std::array<ComPtr<ID3D12CommandAllocator>, kFramesInFlight> m_allocators;
    ComPtr<ID3D12GraphicsCommandList> m_commandList;
    ComPtr<ID3D12CommandAllocator> m_immediateAllocator;
    ComPtr<ID3D12GraphicsCommandList> m_immediateList;
    std::array<ComPtr<ID3D12Resource>, kFramesInFlight> m_frameCB;
    std::array<u8*, kFramesInFlight> m_frameCBMapped{};
    ComPtr<ID3D12Resource> m_uiBuffer;
    u8* m_uiMapped = nullptr;
    D3D12_VERTEX_BUFFER_VIEW m_uiVB{};
    u64 m_uiCapacity = 0;
    ComPtr<ID3D12Fence> m_fence;
    std::array<u64, kFramesInFlight> m_frameFenceValues{};
    u64 m_fenceValue = 0;
    HANDLE m_fenceEvent = nullptr;
    u32 m_frameIndex = 0;
    u32 m_frameSlot = 0;
    u32 m_width = 0;
    u32 m_height = 0;
    u32 m_srvDescriptorSize = 0;
    bool m_vsync = true;
    bool m_debug = false;
    std::string m_adapterName;
    Stats m_stats;
    std::vector<MeshGPU> m_meshes;
    TextureGPU m_textureArray;
    TextureGPU m_fontTexture;
    ComPtr<ID3D12RootSignature> m_sceneRoot;
    ComPtr<ID3D12RootSignature> m_shadowRoot;
    ComPtr<ID3D12RootSignature> m_uiRoot;
    ComPtr<ID3D12PipelineState> m_scenePSO;
    ComPtr<ID3D12PipelineState> m_shadowPSO;
    ComPtr<ID3D12PipelineState> m_skyPSO;
    ComPtr<ID3D12PipelineState> m_uiPSO;
    ComPtr<ID3D12InfoQueue> m_infoQueue;
    u32 m_frameCount = 0;
};

} // namespace

std::unique_ptr<Device> createDX12Device() {
    return std::make_unique<DX12Device>();
}

} // namespace vd::rhi
