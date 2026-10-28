#include "void/rhi/RHI.h"

#include "void/core/Log.h"
#include "void/core/Paths.h"

#include <SDL3/SDL.h>
#include <SDL3/SDL_vulkan.h>
#include <volk.h>

#include <array>
#include <cstring>
#include <fstream>
#include <string>
#include <vector>

namespace vd::rhi {
namespace {

constexpr u32 kFramesInFlight = 2;
constexpr u32 kShadowSize = 2048;
constexpr VkFormat kShadowFormat = VK_FORMAT_R32_SFLOAT;

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

struct Buffer {
    VkBuffer buffer = VK_NULL_HANDLE;
    VkDeviceMemory memory = VK_NULL_HANDLE;
    VkDeviceSize size = 0;
    void* mapped = nullptr;
};

struct MeshGPU {
    Buffer vb;
    Buffer ib;
    u32 indexCount = 0;
};

struct ImageGPU {
    VkImage image = VK_NULL_HANDLE;
    VkDeviceMemory memory = VK_NULL_HANDLE;
    VkImageView view = VK_NULL_HANDLE;
};

std::vector<u8> readFile(const std::filesystem::path& path) {
    std::ifstream file(path, std::ios::binary | std::ios::ate);
    if (!file) {
        log::error("Vulkan: cannot open shader %s", path.string().c_str());
        return {};
    }
    const std::streamsize size = file.tellg();
    file.seekg(0);
    std::vector<u8> data(static_cast<size_t>(size));
    file.read(reinterpret_cast<char*>(data.data()), size);
    return data;
}

const char* vkResultName(VkResult result) {
    switch (result) {
    case VK_SUCCESS: return "VK_SUCCESS";
    case VK_NOT_READY: return "VK_NOT_READY";
    case VK_TIMEOUT: return "VK_TIMEOUT";
    case VK_ERROR_OUT_OF_HOST_MEMORY: return "VK_ERROR_OUT_OF_HOST_MEMORY";
    case VK_ERROR_OUT_OF_DEVICE_MEMORY: return "VK_ERROR_OUT_OF_DEVICE_MEMORY";
    case VK_ERROR_INITIALIZATION_FAILED: return "VK_ERROR_INITIALIZATION_FAILED";
    case VK_ERROR_DEVICE_LOST: return "VK_ERROR_DEVICE_LOST";
    case VK_ERROR_MEMORY_MAP_FAILED: return "VK_ERROR_MEMORY_MAP_FAILED";
    case VK_ERROR_LAYER_NOT_PRESENT: return "VK_ERROR_LAYER_NOT_PRESENT";
    case VK_ERROR_EXTENSION_NOT_PRESENT: return "VK_ERROR_EXTENSION_NOT_PRESENT";
    case VK_ERROR_FEATURE_NOT_PRESENT: return "VK_ERROR_FEATURE_NOT_PRESENT";
    case VK_ERROR_INCOMPATIBLE_DRIVER: return "VK_ERROR_INCOMPATIBLE_DRIVER";
    case VK_ERROR_FORMAT_NOT_SUPPORTED: return "VK_ERROR_FORMAT_NOT_SUPPORTED";
    case VK_ERROR_SURFACE_LOST_KHR: return "VK_ERROR_SURFACE_LOST_KHR";
    case VK_ERROR_OUT_OF_DATE_KHR: return "VK_ERROR_OUT_OF_DATE_KHR";
    case VK_SUBOPTIMAL_KHR: return "VK_SUBOPTIMAL_KHR";
    default: return "VK_ERROR";
    }
}

VKAPI_ATTR VkBool32 VKAPI_CALL debugCallback(
    VkDebugUtilsMessageSeverityFlagBitsEXT severity,
    VkDebugUtilsMessageTypeFlagsEXT,
    const VkDebugUtilsMessengerCallbackDataEXT* data,
    void*) {
    if (severity >= VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT) {
        log::error("Vulkan: %s", data->pMessage);
    } else if (severity >= VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT) {
        log::warn("Vulkan: %s", data->pMessage);
    } else {
        log::debug("Vulkan: %s", data->pMessage);
    }
    return VK_FALSE;
}

class VulkanDevice final : public Device {
public:
    ~VulkanDevice() override { shutdown(); }

    bool initialize(const DeviceDesc& desc) override {
        m_width = desc.width;
        m_height = desc.height;
        m_vsync = desc.vsync;

        if (!SDL_Vulkan_LoadLibrary(nullptr)) {
            log::error("SDL_Vulkan_LoadLibrary failed: %s", SDL_GetError());
            return false;
        }
        m_loaderLoaded = true;
        if (volkInitialize() != VK_SUCCESS) {
            log::error("Vulkan loader not available");
            return false;
        }

        log::debug("Vulkan: instance");
        if (!createInstance(desc)) return false;
        log::debug("Vulkan: surface");
        if (!createSurface(desc.window, desc.adapter)) return false;
        log::debug("Vulkan: pick device");
        if (!pickPhysicalDevice()) return false;
        log::debug("Vulkan: device");
        if (!createDevice()) return false;
        log::debug("Vulkan: swapchain");
        if (!createSwapchain()) return false;
        log::debug("Vulkan: depth");
        if (!createDepthBuffer()) return false;
        log::debug("Vulkan: shadow");
        if (!createShadowMap()) return false;
        log::debug("Vulkan: commands");
        if (!createCommands()) return false;
        log::debug("Vulkan: frame resources");
        if (!createFrameResources()) return false;
        log::debug("Vulkan: descriptors");
        if (!createDescriptorInfrastructure()) return false;
        {
            rhi::ImageData white;
            white.width = 1;
            white.height = 1;
            white.pixels = {255, 255, 255, 255};
            if (!createViewmodelTexture(white)) return false;
        }
        log::debug("Vulkan: pipelines");
        if (!createPipelines()) return false;

        log::info("Vulkan adapter: %s", m_adapterName.c_str());
        log::info("Vulkan API: %u.%u.%u", VK_API_VERSION_MAJOR(m_apiVersion),
                  VK_API_VERSION_MINOR(m_apiVersion), VK_API_VERSION_PATCH(m_apiVersion));
        log::info("Vulkan validation: %s", m_validation ? "on" : "off");
        return true;
    }

    void shutdown() override {
        if (!m_device && !m_instance) return;
        if (m_device) {
            vkDeviceWaitIdle(m_device);
            destroySwapchain();
            if (m_commandPool) {
                vkDestroyCommandPool(m_device, m_commandPool, nullptr);
                m_commandPool = VK_NULL_HANDLE;
            }
            for (u32 i = 0; i < kFramesInFlight; ++i) {
                if (m_imageAvailable[i]) vkDestroySemaphore(m_device, m_imageAvailable[i], nullptr);
                if (m_renderFinished[i]) vkDestroySemaphore(m_device, m_renderFinished[i], nullptr);
                if (m_inFlight[i]) vkDestroyFence(m_device, m_inFlight[i], nullptr);
                m_imageAvailable[i] = VK_NULL_HANDLE;
                m_renderFinished[i] = VK_NULL_HANDLE;
                m_inFlight[i] = VK_NULL_HANDLE;
                if (m_frameUniformMapped[i]) {
                    vkUnmapMemory(m_device, m_frameUniform[i].memory);
                    m_frameUniformMapped[i] = nullptr;
                }
                destroyBuffer(m_frameUniform[i]);
            }
            if (m_uiMapped) {
                vkUnmapMemory(m_device, m_uiBuffer.memory);
                m_uiMapped = nullptr;
            }
            destroyBuffer(m_uiBuffer);
            for (auto& mesh : m_meshes) {
                destroyBuffer(mesh.vb);
                destroyBuffer(mesh.ib);
            }
            m_meshes.clear();
            destroyImage(m_textureArray);
            destroyImage(m_font);
            destroyImage(m_viewTexture);
            destroyImage(m_shadowColor);
            destroyImage(m_shadowImage);
            destroyImage(m_depthImage);
            if (m_textureSampler) vkDestroySampler(m_device, m_textureSampler, nullptr);
            if (m_shadowSampler) vkDestroySampler(m_device, m_shadowSampler, nullptr);
            if (m_fontSampler) vkDestroySampler(m_device, m_fontSampler, nullptr);
            if (m_scenePipeline) vkDestroyPipeline(m_device, m_scenePipeline, nullptr);
            if (m_viewmodelPipeline) vkDestroyPipeline(m_device, m_viewmodelPipeline, nullptr);
            if (m_shadowPipeline) vkDestroyPipeline(m_device, m_shadowPipeline, nullptr);
            if (m_skyPipeline) vkDestroyPipeline(m_device, m_skyPipeline, nullptr);
            if (m_uiPipeline) vkDestroyPipeline(m_device, m_uiPipeline, nullptr);
            if (m_scenePipelineLayout) vkDestroyPipelineLayout(m_device, m_scenePipelineLayout, nullptr);
            if (m_shadowPipelineLayout) vkDestroyPipelineLayout(m_device, m_shadowPipelineLayout, nullptr);
            if (m_skyPipelineLayout) vkDestroyPipelineLayout(m_device, m_skyPipelineLayout, nullptr);
            if (m_uiPipelineLayout) vkDestroyPipelineLayout(m_device, m_uiPipelineLayout, nullptr);
            if (m_sceneSetLayout) vkDestroyDescriptorSetLayout(m_device, m_sceneSetLayout, nullptr);
            if (m_textureSetLayout) vkDestroyDescriptorSetLayout(m_device, m_textureSetLayout, nullptr);
            if (m_uiSetLayout) vkDestroyDescriptorSetLayout(m_device, m_uiSetLayout, nullptr);
            if (m_descriptorPool) vkDestroyDescriptorPool(m_device, m_descriptorPool, nullptr);
            vkDestroyDevice(m_device, nullptr);
            m_device = VK_NULL_HANDLE;
        }
        if (m_surface && m_instance) {
            vkDestroySurfaceKHR(m_instance, m_surface, nullptr);
            m_surface = VK_NULL_HANDLE;
        }
        if (m_debugMessenger && m_instance) {
            auto destroy = reinterpret_cast<PFN_vkDestroyDebugUtilsMessengerEXT>(
                vkGetInstanceProcAddr(m_instance, "vkDestroyDebugUtilsMessengerEXT"));
            if (destroy) destroy(m_instance, m_debugMessenger, nullptr);
            m_debugMessenger = VK_NULL_HANDLE;
        }
        if (m_instance) vkDestroyInstance(m_instance, nullptr);
        m_instance = VK_NULL_HANDLE;
        if (m_loaderLoaded) {
            SDL_Vulkan_UnloadLibrary();
            m_loaderLoaded = false;
        }
    }

    void resize(u32 width, u32 height) override {
        if (!m_device || width == 0 || height == 0) return;
        if (width == m_width && height == m_height) return;
        m_width = width;
        m_height = height;
        vkDeviceWaitIdle(m_device);
        recreateSwapchain();
    }

    void waitIdle() override {
        if (m_device) vkDeviceWaitIdle(m_device);
    }

    MeshHandle createMesh(const MeshData& mesh) override {
        if (mesh.vertices.empty() || mesh.indices.empty()) return {};
        MeshGPU gpu;
        if (!createBufferWithData(reinterpret_cast<const void*>(mesh.vertices.data()),
                                  mesh.vertices.size() * sizeof(Vertex),
                                  VK_BUFFER_USAGE_VERTEX_BUFFER_BIT, gpu.vb)) return {};
        if (!createBufferWithData(reinterpret_cast<const void*>(mesh.indices.data()),
                                  mesh.indices.size() * sizeof(u32),
                                  VK_BUFFER_USAGE_INDEX_BUFFER_BIT, gpu.ib)) return {};
        gpu.indexCount = static_cast<u32>(mesh.indices.size());
        m_meshes.push_back(gpu);
        return MeshHandle{static_cast<u32>(m_meshes.size())};
    }

    bool createTextureArray(const std::vector<ImageData>& layers) override {
        if (layers.empty()) return false;
        const u32 width = layers[0].width;
        const u32 height = layers[0].height;
        for (const auto& layer : layers) {
            if (layer.width != width || layer.height != height) return false;
        }

        destroyImage(m_textureArray);
        if (!createImage(width, height, static_cast<u32>(layers.size()), VK_FORMAT_R8G8B8A8_SRGB,
                         VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT, m_textureArray)) {
            return false;
        }

        Buffer staging;
        const VkDeviceSize layerSize = static_cast<VkDeviceSize>(width) * height * 4;
        if (!createBuffer(layerSize * layers.size(), VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
                          VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                          staging, true)) return false;
        for (u32 layer = 0; layer < layers.size(); ++layer) {
            std::memcpy(static_cast<u8*>(staging.mapped) + layer * layerSize,
                        layers[layer].pixels.data(), layerSize);
        }

        VkCommandBuffer cmd = beginImmediate();
        if (cmd == VK_NULL_HANDLE) return false;
        transitionImage(cmd, m_textureArray.image, VK_IMAGE_LAYOUT_UNDEFINED,
                        VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, VK_IMAGE_ASPECT_COLOR_BIT, 0, 0,
                        VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT,
                        static_cast<u32>(layers.size()));
        std::vector<VkBufferImageCopy> copies(layers.size());
        for (u32 i = 0; i < layers.size(); ++i) {
            copies[i].bufferOffset = layerSize * i;
            copies[i].imageSubresource.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
            copies[i].imageSubresource.mipLevel = 0;
            copies[i].imageSubresource.baseArrayLayer = i;
            copies[i].imageSubresource.layerCount = 1;
            copies[i].imageExtent = {width, height, 1};
        }
        vkCmdCopyBufferToImage(cmd, staging.buffer, m_textureArray.image,
                               VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                               static_cast<u32>(copies.size()), copies.data());
        transitionImage(cmd, m_textureArray.image, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                        VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL, VK_IMAGE_ASPECT_COLOR_BIT, 0, 0,
                        VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT,
                        static_cast<u32>(layers.size()));
        endImmediate(cmd);
        destroyBuffer(staging);

        VkImageViewCreateInfo view{};
        view.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO;
        view.image = m_textureArray.image;
        view.viewType = VK_IMAGE_VIEW_TYPE_2D_ARRAY;
        view.format = VK_FORMAT_R8G8B8A8_SRGB;
        view.subresourceRange.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
        view.subresourceRange.baseMipLevel = 0;
        view.subresourceRange.levelCount = 1;
        view.subresourceRange.baseArrayLayer = 0;
        view.subresourceRange.layerCount = static_cast<u32>(layers.size());
        if (vkCreateImageView(m_device, &view, nullptr, &m_textureArray.view) != VK_SUCCESS) return false;

        VkDescriptorImageInfo imageInfo{};
        imageInfo.imageLayout = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL;
        imageInfo.imageView = m_textureArray.view;
        imageInfo.sampler = m_textureSampler;
        VkWriteDescriptorSet write{};
        write.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET;
        write.dstSet = m_textureSet;
        write.dstBinding = 0;
        write.descriptorCount = 1;
        write.descriptorType = VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE;
        write.pImageInfo = &imageInfo;
        VkDescriptorImageInfo samplerInfo{};
        samplerInfo.sampler = m_textureSampler;
        VkWriteDescriptorSet samplerWrite{};
        samplerWrite.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET;
        samplerWrite.dstSet = m_textureSet;
        samplerWrite.dstBinding = 1;
        samplerWrite.descriptorCount = 1;
        samplerWrite.descriptorType = VK_DESCRIPTOR_TYPE_SAMPLER;
        samplerWrite.pImageInfo = &samplerInfo;
        VkWriteDescriptorSet writes[] = {write, samplerWrite};
        vkUpdateDescriptorSets(m_device, 2, writes, 0, nullptr);
        return true;
    }

    bool createFontTexture(const ImageData& image) override {
        if (image.pixels.empty()) return false;
        destroyImage(m_font);
        if (!createImage(image.width, image.height, 1, VK_FORMAT_R8_UNORM,
                         VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT, m_font)) return false;

        Buffer staging;
        const VkDeviceSize size = static_cast<VkDeviceSize>(image.width) * image.height;
        if (!createBuffer(size, VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
                          VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                          staging, true)) return false;
        std::memcpy(staging.mapped, image.pixels.data(), size);

        VkCommandBuffer cmd = beginImmediate();
        if (cmd == VK_NULL_HANDLE) return false;
        transitionImage(cmd, m_font.image, VK_IMAGE_LAYOUT_UNDEFINED,
                        VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, VK_IMAGE_ASPECT_COLOR_BIT, 0, 0,
                        VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT, 1);
        VkBufferImageCopy copy{};
        copy.imageSubresource.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
        copy.imageSubresource.layerCount = 1;
        copy.imageExtent = {image.width, image.height, 1};
        vkCmdCopyBufferToImage(cmd, staging.buffer, m_font.image, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, 1, &copy);
        transitionImage(cmd, m_font.image, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                        VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL, VK_IMAGE_ASPECT_COLOR_BIT, 0, 0,
                        VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT, 1);
        endImmediate(cmd);
        destroyBuffer(staging);

        VkImageViewCreateInfo view{};
        view.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO;
        view.image = m_font.image;
        view.viewType = VK_IMAGE_VIEW_TYPE_2D;
        view.format = VK_FORMAT_R8_UNORM;
        view.subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
        if (vkCreateImageView(m_device, &view, nullptr, &m_font.view) != VK_SUCCESS) return false;

        VkDescriptorImageInfo info{};
        info.imageLayout = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL;
        info.imageView = m_font.view;
        info.sampler = m_fontSampler;
        VkWriteDescriptorSet write{};
        write.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET;
        write.dstSet = m_uiSet;
        write.dstBinding = 0;
        write.descriptorCount = 1;
        write.descriptorType = VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE;
        write.pImageInfo = &info;
        vkUpdateDescriptorSets(m_device, 1, &write, 0, nullptr);
        VkDescriptorImageInfo samplerInfo{};
        samplerInfo.sampler = m_fontSampler;
        VkWriteDescriptorSet samplerWrite{};
        samplerWrite.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET;
        samplerWrite.dstSet = m_uiSet;
        samplerWrite.dstBinding = 1;
        samplerWrite.descriptorCount = 1;
        samplerWrite.descriptorType = VK_DESCRIPTOR_TYPE_SAMPLER;
        samplerWrite.pImageInfo = &samplerInfo;
        vkUpdateDescriptorSets(m_device, 1, &samplerWrite, 0, nullptr);
        return true;
    }

    bool createViewmodelTexture(const ImageData& image) override {
        if (image.pixels.empty() || image.width == 0 || image.height == 0) return false;
        destroyImage(m_viewTexture);
        if (!createImage(image.width, image.height, 1, VK_FORMAT_R8G8B8A8_SRGB,
                         VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT,
                         m_viewTexture)) return false;

        Buffer staging;
        const VkDeviceSize size = static_cast<VkDeviceSize>(image.width) * image.height * 4;
        if (!createBuffer(size, VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
                          VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                          staging, true)) return false;
        std::memcpy(staging.mapped, image.pixels.data(), static_cast<size_t>(size));

        VkCommandBuffer cmd = beginImmediate();
        if (cmd == VK_NULL_HANDLE) return false;
        transitionImage(cmd, m_viewTexture.image, VK_IMAGE_LAYOUT_UNDEFINED,
                        VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, VK_IMAGE_ASPECT_COLOR_BIT, 0, 0,
                        VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT, 1);
        VkBufferImageCopy copy{};
        copy.imageSubresource.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
        copy.imageSubresource.layerCount = 1;
        copy.imageExtent = {image.width, image.height, 1};
        vkCmdCopyBufferToImage(cmd, staging.buffer, m_viewTexture.image,
                               VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, 1, &copy);
        transitionImage(cmd, m_viewTexture.image, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                        VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL, VK_IMAGE_ASPECT_COLOR_BIT, 0, 0,
                        VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT, 1);
        endImmediate(cmd);
        destroyBuffer(staging);

        VkImageViewCreateInfo view{};
        view.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO;
        view.image = m_viewTexture.image;
        view.viewType = VK_IMAGE_VIEW_TYPE_2D;
        view.format = VK_FORMAT_R8G8B8A8_SRGB;
        view.subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
        if (vkCreateImageView(m_device, &view, nullptr, &m_viewTexture.view) != VK_SUCCESS) return false;

        VkDescriptorImageInfo info{};
        info.imageLayout = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL;
        info.imageView = m_viewTexture.view;
        VkWriteDescriptorSet write{};
        write.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET;
        write.dstSet = m_textureSet;
        write.dstBinding = 2;
        write.descriptorCount = 1;
        write.descriptorType = VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE;
        write.pImageInfo = &info;
        vkUpdateDescriptorSets(m_device, 1, &write, 0, nullptr);
        return true;
    }

    void render(const FrameData& frame, const DrawList& draws, const UIList& ui) override {
        if (!m_scenePipeline || !m_swapchain) return;
        m_stats = {};
        vkWaitForFences(m_device, 1, &m_inFlight[m_frameSlot], VK_TRUE, UINT64_MAX);

        u32 imageIndex = 0;
        VkResult acquire = vkAcquireNextImageKHR(m_device, m_swapchain, UINT64_MAX,
                                                 m_imageAvailable[m_frameSlot], VK_NULL_HANDLE, &imageIndex);
        if (acquire == VK_ERROR_OUT_OF_DATE_KHR) {
            recreateSwapchain();
            return;
        }
        if (acquire != VK_SUCCESS && acquire != VK_SUBOPTIMAL_KHR) return;
        if (acquire == VK_SUBOPTIMAL_KHR) m_swapchainDirty = true;

        vkResetFences(m_device, 1, &m_inFlight[m_frameSlot]);
        std::memcpy(m_frameUniformMapped[m_frameSlot], &frame, sizeof(FrameData));
        if (!ui.vertices.empty()) {
            const u64 bytes = static_cast<u64>(ui.vertices.size() * sizeof(UIVertex));
            if (bytes <= m_uiBuffer.size) std::memcpy(m_uiMapped, ui.vertices.data(), bytes);
        }

        vkResetCommandBuffer(m_commandBuffers[m_frameSlot], 0);
        VkCommandBuffer cmd = m_commandBuffers[m_frameSlot];
        VkCommandBufferBeginInfo begin{};
        begin.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO;
        begin.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
        vkBeginCommandBuffer(cmd, &begin);

        // Shadow pass.
        transitionImage(cmd, m_shadowImage.image, VK_IMAGE_LAYOUT_UNDEFINED,
                        VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL, VK_IMAGE_ASPECT_DEPTH_BIT, 0, 0,
                        VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_EARLY_FRAGMENT_TESTS_BIT, 1);
        transitionImage(cmd, m_shadowColor.image, VK_IMAGE_LAYOUT_UNDEFINED,
                        VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL, VK_IMAGE_ASPECT_COLOR_BIT, 0, 0,
                        VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT, 1);
        VkRenderingAttachmentInfo shadowDepth{};
        shadowDepth.sType = VK_STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO;
        shadowDepth.imageView = m_shadowImage.view;
        shadowDepth.imageLayout = VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL;
        shadowDepth.loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR;
        shadowDepth.storeOp = VK_ATTACHMENT_STORE_OP_STORE;
        shadowDepth.clearValue.depthStencil = {1.0f, 0};
        VkRenderingAttachmentInfo shadowColor{};
        shadowColor.sType = VK_STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO;
        shadowColor.imageView = m_shadowColor.view;
        shadowColor.imageLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL;
        shadowColor.loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR;
        shadowColor.storeOp = VK_ATTACHMENT_STORE_OP_STORE;
        shadowColor.clearValue.color = {{1.0f, 1.0f, 1.0f, 1.0f}};
        VkRenderingInfo shadowRendering{};
        shadowRendering.sType = VK_STRUCTURE_TYPE_RENDERING_INFO;
        shadowRendering.renderArea = {{0, 0}, {kShadowSize, kShadowSize}};
        shadowRendering.layerCount = 1;
        shadowRendering.colorAttachmentCount = 1;
        shadowRendering.pColorAttachments = &shadowColor;
        shadowRendering.pDepthAttachment = &shadowDepth;
        vkCmdBeginRendering(cmd, &shadowRendering);
        VkViewport shadowViewport{0.0f, static_cast<float>(kShadowSize), static_cast<float>(kShadowSize),
                                  -static_cast<float>(kShadowSize), 0.0f, 1.0f};
        VkRect2D shadowScissor{{0, 0}, {kShadowSize, kShadowSize}};
        vkCmdSetViewport(cmd, 0, 1, &shadowViewport);
        vkCmdSetScissor(cmd, 0, 1, &shadowScissor);
        vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, m_shadowPipeline);
        for (const auto& item : draws.items) {
            if (!item.castShadow || item.mesh.id == 0 || item.mesh.id > m_meshes.size()) continue;
            const MeshGPU& mesh = m_meshes[item.mesh.id - 1];
            VkDeviceSize offset = 0;
            vkCmdBindVertexBuffers(cmd, 0, 1, &mesh.vb.buffer, &offset);
            vkCmdBindIndexBuffer(cmd, mesh.ib.buffer, 0, VK_INDEX_TYPE_UINT32);
            ShadowPush push;
            push.viewProj = draws.shadowViewProj;
            push.model = item.transform;
            vkCmdPushConstants(cmd, m_shadowPipelineLayout, VK_SHADER_STAGE_VERTEX_BIT, 0, sizeof(ShadowPush), &push);
            vkCmdDrawIndexed(cmd, mesh.indexCount, 1, 0, 0, 0);
            ++m_stats.drawCalls;
        }
        vkCmdEndRendering(cmd);
        transitionImage(cmd, m_shadowColor.image, VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
                        VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL, VK_IMAGE_ASPECT_COLOR_BIT, 0, 0,
                        VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT, VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT, 1);

        // Main pass.
        transitionImage(cmd, m_swapchainImages[imageIndex], VK_IMAGE_LAYOUT_UNDEFINED,
                        VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL, VK_IMAGE_ASPECT_COLOR_BIT, 0, 0,
                        VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT, 1);
        transitionImage(cmd, m_depthImage.image, VK_IMAGE_LAYOUT_UNDEFINED,
                        VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL, VK_IMAGE_ASPECT_DEPTH_BIT, 0, 0,
                        VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_EARLY_FRAGMENT_TESTS_BIT, 1);

        VkRenderingAttachmentInfo color{};
        color.sType = VK_STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO;
        color.imageView = m_swapchainViews[imageIndex];
        color.imageLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL;
        color.loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR;
        color.storeOp = VK_ATTACHMENT_STORE_OP_STORE;
        color.clearValue.color = {{0.035f, 0.045f, 0.055f, 1.0f}};
        VkRenderingAttachmentInfo depth{};
        depth.sType = VK_STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO;
        depth.imageView = m_depthImage.view;
        depth.imageLayout = VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL;
        depth.loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR;
        depth.storeOp = VK_ATTACHMENT_STORE_OP_STORE;
        depth.clearValue.depthStencil = {1.0f, 0};
        VkRenderingInfo rendering{};
        rendering.sType = VK_STRUCTURE_TYPE_RENDERING_INFO;
        rendering.renderArea = {{0, 0}, m_swapchainExtent};
        rendering.layerCount = 1;
        rendering.colorAttachmentCount = 1;
        rendering.pColorAttachments = &color;
        rendering.pDepthAttachment = &depth;
        vkCmdBeginRendering(cmd, &rendering);

        VkViewport viewport{0.0f, static_cast<float>(m_swapchainExtent.height),
                            static_cast<float>(m_swapchainExtent.width),
                            -static_cast<float>(m_swapchainExtent.height), 0.0f, 1.0f};
        VkRect2D scissor{{0, 0}, m_swapchainExtent};
        vkCmdSetViewport(cmd, 0, 1, &viewport);
        vkCmdSetScissor(cmd, 0, 1, &scissor);

        vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, m_skyPipeline);
        vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, m_skyPipelineLayout, 0, 1,
                                &m_sceneSets[m_frameSlot], 0, nullptr);
        vkCmdDraw(cmd, 3, 1, 0, 0);
        ++m_stats.drawCalls;

        if (m_textureArray.view) {
            VkDescriptorSet sceneSets[] = {m_sceneSets[m_frameSlot], m_textureSet};
            vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, m_scenePipelineLayout, 0, 2,
                                    sceneSets, 0, nullptr);
            const auto drawSceneItem = [&](const DrawItem& item) {
                if (item.mesh.id == 0 || item.mesh.id > m_meshes.size()) return;
                const MeshGPU& mesh = m_meshes[item.mesh.id - 1];
                VkDeviceSize offset = 0;
                vkCmdBindVertexBuffers(cmd, 0, 1, &mesh.vb.buffer, &offset);
                vkCmdBindIndexBuffer(cmd, mesh.ib.buffer, 0, VK_INDEX_TYPE_UINT32);
                PushData push;
                push.model = item.transform;
                push.baseColor = item.baseColor;
                push.material = {item.roughness, item.metallic, item.emissive, static_cast<float>(item.textureLayer)};
                push.uvScale = {item.uvScale.x, item.uvScale.y, item.unlit ? 1.0f : 0.0f, 0.0f};
                vkCmdPushConstants(cmd, m_scenePipelineLayout, VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT,
                                   0, sizeof(PushData), &push);
                vkCmdDrawIndexed(cmd, mesh.indexCount, 1, 0, 0, 0);
                ++m_stats.drawCalls;
                m_stats.triangles += mesh.indexCount / 3;
            };
            vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, m_scenePipeline);
            for (const auto& item : draws.items) {
                if (!item.viewmodel) drawSceneItem(item);
            }
            vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, m_viewmodelPipeline);
            for (const auto& item : draws.items) {
                if (item.viewmodel) drawSceneItem(item);
            }
        }

        if (!ui.vertices.empty() && m_font.view) {
            VkDeviceSize offset = 0;
            vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, m_uiPipeline);
            vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, m_uiPipelineLayout, 0, 1,
                                    &m_uiSet, 0, nullptr);
            vkCmdBindVertexBuffers(cmd, 0, 1, &m_uiBuffer.buffer, &offset);
            UIPush push;
            push.ortho = glm::ortho(0.0f, static_cast<float>(m_swapchainExtent.width),
                                    static_cast<float>(m_swapchainExtent.height), 0.0f, -1.0f, 1.0f);
            vkCmdPushConstants(cmd, m_uiPipelineLayout, VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT,
                               0, sizeof(UIPush), &push);
            vkCmdDraw(cmd, static_cast<u32>(ui.vertices.size()), 1, 0, 0);
            ++m_stats.drawCalls;
        }

        vkCmdEndRendering(cmd);
        transitionImage(cmd, m_swapchainImages[imageIndex], VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
                        VK_IMAGE_LAYOUT_PRESENT_SRC_KHR, VK_IMAGE_ASPECT_COLOR_BIT, 0, 0,
                        VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT, VK_PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT, 1);

        vkEndCommandBuffer(cmd);
        VkPipelineStageFlags waitStage = VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT;
        VkSubmitInfo submit{};
        submit.sType = VK_STRUCTURE_TYPE_SUBMIT_INFO;
        submit.waitSemaphoreCount = 1;
        submit.pWaitSemaphores = &m_imageAvailable[m_frameSlot];
        submit.pWaitDstStageMask = &waitStage;
        submit.commandBufferCount = 1;
        submit.pCommandBuffers = &cmd;
        submit.signalSemaphoreCount = 1;
        submit.pSignalSemaphores = &m_renderFinished[m_frameSlot];
        if (vkQueueSubmit(m_graphicsQueue, 1, &submit, m_inFlight[m_frameSlot]) != VK_SUCCESS) return;

        VkPresentInfoKHR present{};
        present.sType = VK_STRUCTURE_TYPE_PRESENT_INFO_KHR;
        present.waitSemaphoreCount = 1;
        present.pWaitSemaphores = &m_renderFinished[m_frameSlot];
        present.swapchainCount = 1;
        present.pSwapchains = &m_swapchain;
        present.pImageIndices = &imageIndex;
        VkResult presentResult = vkQueuePresentKHR(m_graphicsQueue, &present);
        if (presentResult == VK_ERROR_OUT_OF_DATE_KHR || presentResult == VK_SUBOPTIMAL_KHR) {
            m_swapchainDirty = true;
        }

        m_frameSlot = (m_frameSlot + 1) % kFramesInFlight;
        if (m_swapchainDirty) {
            m_swapchainDirty = false;
            recreateSwapchain();
        }
    }

    Api api() const override { return Api::Vulkan; }
    const char* adapterName() const override { return m_adapterName.c_str(); }
    const Stats& stats() const override { return m_stats; }

private:
    bool createInstance(const DeviceDesc& desc) {
        std::vector<const char*> extensions;
        u32 sdlCount = 0;
        const char* const* sdlExtensions = SDL_Vulkan_GetInstanceExtensions(&sdlCount);
        for (u32 i = 0; i < sdlCount; ++i) extensions.push_back(sdlExtensions[i]);

        if (desc.validation) {
            u32 layerCount = 0;
            vkEnumerateInstanceLayerProperties(&layerCount, nullptr);
            std::vector<VkLayerProperties> layers(layerCount);
            vkEnumerateInstanceLayerProperties(&layerCount, layers.data());
            for (const auto& layer : layers) {
                if (std::strcmp(layer.layerName, "VK_LAYER_KHRONOS_validation") == 0) {
                    m_validation = true;
                    break;
                }
            }
            if (m_validation) extensions.push_back(VK_EXT_DEBUG_UTILS_EXTENSION_NAME);
            else log::warn("Vulkan validation layers requested but not installed");
        }

        VkApplicationInfo app{};
        app.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO;
        app.pApplicationName = desc.appName;
        app.applicationVersion = VK_MAKE_VERSION(0, 1, 0);
        app.pEngineName = "V.O.I.D. Engine";
        app.engineVersion = VK_MAKE_VERSION(0, 1, 0);
        app.apiVersion = VK_API_VERSION_1_3;

        VkInstanceCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO;
        info.pApplicationInfo = &app;
        info.enabledExtensionCount = static_cast<u32>(extensions.size());
        info.ppEnabledExtensionNames = extensions.data();
        if (m_validation) {
            const char* layers[] = {"VK_LAYER_KHRONOS_validation"};
            info.enabledLayerCount = 1;
            info.ppEnabledLayerNames = layers;
        }
        VkResult result = vkCreateInstance(&info, nullptr, &m_instance);
        if (result != VK_SUCCESS) {
            log::error("vkCreateInstance failed: %s", vkResultName(result));
            return false;
        }
        volkLoadInstance(m_instance);

        if (m_validation) {
            VkDebugUtilsMessengerCreateInfoEXT debug{};
            debug.sType = VK_STRUCTURE_TYPE_DEBUG_UTILS_MESSENGER_CREATE_INFO_EXT;
            debug.messageSeverity = VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT |
                                    VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT;
            debug.messageType = VK_DEBUG_UTILS_MESSAGE_TYPE_GENERAL_BIT_EXT |
                                VK_DEBUG_UTILS_MESSAGE_TYPE_VALIDATION_BIT_EXT |
                                VK_DEBUG_UTILS_MESSAGE_TYPE_PERFORMANCE_BIT_EXT;
            debug.pfnUserCallback = debugCallback;
            auto create = reinterpret_cast<PFN_vkCreateDebugUtilsMessengerEXT>(
                vkGetInstanceProcAddr(m_instance, "vkCreateDebugUtilsMessengerEXT"));
            if (create) create(m_instance, &debug, nullptr, &m_debugMessenger);
        }
        return true;
    }

    bool createSurface(SDL_Window* window, int requestedAdapter) {
        if (!SDL_Vulkan_CreateSurface(window, m_instance, nullptr, &m_surface)) {
            log::error("SDL_Vulkan_CreateSurface failed: %s", SDL_GetError());
            return false;
        }
        m_requestedAdapter = requestedAdapter;
        return true;
    }

    bool pickPhysicalDevice() {
        u32 count = 0;
        vkEnumeratePhysicalDevices(m_instance, &count, nullptr);
        if (count == 0) return false;
        std::vector<VkPhysicalDevice> devices(count);
        vkEnumeratePhysicalDevices(m_instance, &count, devices.data());

        VkPhysicalDevice best = VK_NULL_HANDLE;
        int bestScore = -1;
        for (u32 i = 0; i < count; ++i) {
            VkPhysicalDeviceProperties props{};
            vkGetPhysicalDeviceProperties(devices[i], &props);
            if (props.apiVersion < VK_API_VERSION_1_3) continue;

            u32 queueCount = 0;
            vkGetPhysicalDeviceQueueFamilyProperties(devices[i], &queueCount, nullptr);
            std::vector<VkQueueFamilyProperties> queues(queueCount);
            vkGetPhysicalDeviceQueueFamilyProperties(devices[i], &queueCount, queues.data());
            int graphics = -1;
            for (u32 q = 0; q < queueCount; ++q) {
                VkBool32 present = VK_FALSE;
                vkGetPhysicalDeviceSurfaceSupportKHR(devices[i], q, m_surface, &present);
                if ((queues[q].queueFlags & VK_QUEUE_GRAPHICS_BIT) && present) {
                    graphics = static_cast<int>(q);
                    break;
                }
            }
            if (graphics < 0) continue;

            u32 extCount = 0;
            vkEnumerateDeviceExtensionProperties(devices[i], nullptr, &extCount, nullptr);
            std::vector<VkExtensionProperties> exts(extCount);
            vkEnumerateDeviceExtensionProperties(devices[i], nullptr, &extCount, exts.data());
            bool swapchainExt = false;
            for (const auto& ext : exts) {
                if (std::strcmp(ext.extensionName, VK_KHR_SWAPCHAIN_EXTENSION_NAME) == 0) swapchainExt = true;
            }
            if (!swapchainExt) continue;

            VkPhysicalDeviceVulkan13Features features{};
            features.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES;
            VkPhysicalDeviceFeatures2 features2{};
            features2.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2;
            features2.pNext = &features;
            vkGetPhysicalDeviceFeatures2(devices[i], &features2);
            if (!features.dynamicRendering) continue;

            if (m_requestedAdapter >= 0 && static_cast<int>(i) != m_requestedAdapter) continue;
            int score = static_cast<int>(props.limits.maxImageDimension2D);
            if (props.deviceType == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU) score += 100000;
            else if (props.deviceType == VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU) score += 50000;
            if (score > bestScore) {
                bestScore = score;
                best = devices[i];
                m_graphicsFamily = static_cast<u32>(graphics);
                m_apiVersion = props.apiVersion;
                m_adapterName = props.deviceName;
            }
        }
        if (!best) {
            log::error("Vulkan: no suitable device (Vulkan 1.3, dynamic rendering, swapchain)");
            return false;
        }
        m_physicalDevice = best;
        return true;
    }

    bool createDevice() {
        float priority = 1.0f;
        VkDeviceQueueCreateInfo queue{};
        queue.sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO;
        queue.queueFamilyIndex = m_graphicsFamily;
        queue.queueCount = 1;
        queue.pQueuePriorities = &priority;
        VkPhysicalDeviceVulkan13Features features{};
        features.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES;
        features.dynamicRendering = VK_TRUE;
        const char* extensions[] = {VK_KHR_SWAPCHAIN_EXTENSION_NAME};
        VkDeviceCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO;
        info.pNext = &features;
        info.queueCreateInfoCount = 1;
        info.pQueueCreateInfos = &queue;
        info.enabledExtensionCount = 1;
        info.ppEnabledExtensionNames = extensions;
        VkResult result = vkCreateDevice(m_physicalDevice, &info, nullptr, &m_device);
        if (result != VK_SUCCESS) {
            log::error("vkCreateDevice failed: %s", vkResultName(result));
            return false;
        }
        volkLoadDevice(m_device);
        vkGetDeviceQueue(m_device, m_graphicsFamily, 0, &m_graphicsQueue);
        return true;
    }

    u32 findMemoryType(u32 typeBits, VkMemoryPropertyFlags flags) const {
        VkPhysicalDeviceMemoryProperties props{};
        vkGetPhysicalDeviceMemoryProperties(m_physicalDevice, &props);
        for (u32 i = 0; i < props.memoryTypeCount; ++i) {
            if ((typeBits & (1u << i)) && (props.memoryTypes[i].propertyFlags & flags) == flags) return i;
        }
        return 0;
    }

    bool createBuffer(VkDeviceSize size, VkBufferUsageFlags usage, VkMemoryPropertyFlags properties,
                      Buffer& out, bool map) {
        VkBufferCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO;
        info.size = size;
        info.usage = usage;
        info.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
        if (vkCreateBuffer(m_device, &info, nullptr, &out.buffer) != VK_SUCCESS) return false;
        VkMemoryRequirements req{};
        vkGetBufferMemoryRequirements(m_device, out.buffer, &req);
        VkMemoryAllocateInfo alloc{};
        alloc.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO;
        alloc.allocationSize = req.size;
        alloc.memoryTypeIndex = findMemoryType(req.memoryTypeBits, properties);
        if (vkAllocateMemory(m_device, &alloc, nullptr, &out.memory) != VK_SUCCESS) return false;
        vkBindBufferMemory(m_device, out.buffer, out.memory, 0);
        out.size = size;
        if (map) vkMapMemory(m_device, out.memory, 0, size, 0, &out.mapped);
        return true;
    }

    bool createBufferWithData(const void* data, VkDeviceSize size, VkBufferUsageFlags usage, Buffer& out) {
        Buffer staging;
        if (!createBuffer(size, VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
                          VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                          staging, true)) return false;
        std::memcpy(staging.mapped, data, size);

        if (!createBuffer(size, usage | VK_BUFFER_USAGE_TRANSFER_DST_BIT,
                          VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, out, false)) return false;

        VkCommandBuffer cmd = beginImmediate();
        if (cmd == VK_NULL_HANDLE) return false;
        VkBufferCopy copy{0, 0, size};
        vkCmdCopyBuffer(cmd, staging.buffer, out.buffer, 1, &copy);
        endImmediate(cmd);
        destroyBuffer(staging);
        return true;
    }

    bool createImage(u32 width, u32 height, u32 layers, VkFormat format, VkImageUsageFlags usage, ImageGPU& out) {
        VkImageCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO;
        info.imageType = VK_IMAGE_TYPE_2D;
        info.extent = {width, height, 1};
        info.mipLevels = 1;
        info.arrayLayers = layers;
        info.format = format;
        info.tiling = VK_IMAGE_TILING_OPTIMAL;
        info.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED;
        info.usage = usage;
        info.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
        info.samples = VK_SAMPLE_COUNT_1_BIT;
        if (vkCreateImage(m_device, &info, nullptr, &out.image) != VK_SUCCESS) return false;
        VkMemoryRequirements req{};
        vkGetImageMemoryRequirements(m_device, out.image, &req);
        VkMemoryAllocateInfo alloc{};
        alloc.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO;
        alloc.allocationSize = req.size;
        alloc.memoryTypeIndex = findMemoryType(req.memoryTypeBits, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT);
        if (vkAllocateMemory(m_device, &alloc, nullptr, &out.memory) != VK_SUCCESS) return false;
        return vkBindImageMemory(m_device, out.image, out.memory, 0) == VK_SUCCESS;
    }

    void destroyBuffer(Buffer& buffer) {
        if (!m_device) return;
        if (buffer.mapped) {
            vkUnmapMemory(m_device, buffer.memory);
            buffer.mapped = nullptr;
        }
        if (buffer.buffer) vkDestroyBuffer(m_device, buffer.buffer, nullptr);
        if (buffer.memory) vkFreeMemory(m_device, buffer.memory, nullptr);
        buffer = {};
    }

    void destroyImage(ImageGPU& image) {
        if (!m_device) return;
        if (image.view) vkDestroyImageView(m_device, image.view, nullptr);
        if (image.image) vkDestroyImage(m_device, image.image, nullptr);
        if (image.memory) vkFreeMemory(m_device, image.memory, nullptr);
        image = {};
    }

    bool createDepthBuffer() {
        destroyImage(m_depthImage);
        if (!createImage(m_swapchainExtent.width, m_swapchainExtent.height, 1, VK_FORMAT_D32_SFLOAT,
                         VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT, m_depthImage)) return false;
        VkImageViewCreateInfo view{};
        view.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO;
        view.image = m_depthImage.image;
        view.viewType = VK_IMAGE_VIEW_TYPE_2D;
        view.format = VK_FORMAT_D32_SFLOAT;
        view.subresourceRange = {VK_IMAGE_ASPECT_DEPTH_BIT, 0, 1, 0, 1};
        return vkCreateImageView(m_device, &view, nullptr, &m_depthImage.view) == VK_SUCCESS;
    }

    VkSampler createSampler(VkFilter filter, VkSamplerAddressMode address, bool compare) {
        VkSamplerCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_SAMPLER_CREATE_INFO;
        info.magFilter = filter;
        info.minFilter = filter;
        info.mipmapMode = VK_SAMPLER_MIPMAP_MODE_LINEAR;
        info.addressModeU = address;
        info.addressModeV = address;
        info.addressModeW = address;
        info.maxAnisotropy = 8.0f;
        info.anisotropyEnable = VK_TRUE;
        info.compareEnable = compare ? VK_TRUE : VK_FALSE;
        info.compareOp = VK_COMPARE_OP_LESS_OR_EQUAL;
        info.borderColor = VK_BORDER_COLOR_FLOAT_OPAQUE_WHITE;
        VkSampler sampler = VK_NULL_HANDLE;
        if (vkCreateSampler(m_device, &info, nullptr, &sampler) != VK_SUCCESS) return VK_NULL_HANDLE;
        return sampler;
    }

    bool createShadowMap() {
        if (!(m_shadowSampler = createSampler(VK_FILTER_LINEAR, VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE, false))) return false;
        if (!(m_textureSampler = createSampler(VK_FILTER_LINEAR, VK_SAMPLER_ADDRESS_MODE_REPEAT, false))) return false;
        if (!(m_fontSampler = createSampler(VK_FILTER_LINEAR, VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE, false))) return false;

        if (!createImage(kShadowSize, kShadowSize, 1, VK_FORMAT_D32_SFLOAT,
                         VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT,
                         m_shadowImage)) return false;
        VkImageViewCreateInfo depthView{};
        depthView.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO;
        depthView.image = m_shadowImage.image;
        depthView.viewType = VK_IMAGE_VIEW_TYPE_2D;
        depthView.format = VK_FORMAT_D32_SFLOAT;
        depthView.subresourceRange = {VK_IMAGE_ASPECT_DEPTH_BIT, 0, 1, 0, 1};
        if (vkCreateImageView(m_device, &depthView, nullptr, &m_shadowImage.view) != VK_SUCCESS) return false;

        if (!createImage(kShadowSize, kShadowSize, 1, kShadowFormat,
                         VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_SAMPLED_BIT,
                         m_shadowColor)) return false;
        VkImageViewCreateInfo colorView{};
        colorView.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO;
        colorView.image = m_shadowColor.image;
        colorView.viewType = VK_IMAGE_VIEW_TYPE_2D;
        colorView.format = kShadowFormat;
        colorView.subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
        return vkCreateImageView(m_device, &colorView, nullptr, &m_shadowColor.view) == VK_SUCCESS;
    }

    bool createSwapchain() {
        VkSurfaceCapabilitiesKHR caps{};
        if (vkGetPhysicalDeviceSurfaceCapabilitiesKHR(m_physicalDevice, m_surface, &caps) != VK_SUCCESS) return false;
        u32 formatCount = 0;
        vkGetPhysicalDeviceSurfaceFormatsKHR(m_physicalDevice, m_surface, &formatCount, nullptr);
        std::vector<VkSurfaceFormatKHR> formats(formatCount);
        vkGetPhysicalDeviceSurfaceFormatsKHR(m_physicalDevice, m_surface, &formatCount, formats.data());
        VkSurfaceFormatKHR chosen = formats.empty() ? VkSurfaceFormatKHR{VK_FORMAT_B8G8R8A8_UNORM, VK_COLOR_SPACE_SRGB_NONLINEAR_KHR} : formats[0];
        for (const auto& format : formats) {
            if (format.format == VK_FORMAT_B8G8R8A8_UNORM && format.colorSpace == VK_COLOR_SPACE_SRGB_NONLINEAR_KHR) {
                chosen = format;
                break;
            }
        }
        u32 modeCount = 0;
        vkGetPhysicalDeviceSurfacePresentModesKHR(m_physicalDevice, m_surface, &modeCount, nullptr);
        std::vector<VkPresentModeKHR> modes(modeCount);
        vkGetPhysicalDeviceSurfacePresentModesKHR(m_physicalDevice, m_surface, &modeCount, modes.data());
        VkPresentModeKHR presentMode = VK_PRESENT_MODE_FIFO_KHR;
        if (!m_vsync) {
            for (VkPresentModeKHR mode : modes) {
                if (mode == VK_PRESENT_MODE_MAILBOX_KHR) {
                    presentMode = mode;
                    break;
                }
            }
        }
        VkExtent2D extent = caps.currentExtent;
        if (extent.width == UINT32_MAX) {
            extent.width = std::max(caps.minImageExtent.width, std::min(caps.maxImageExtent.width, m_width));
            extent.height = std::max(caps.minImageExtent.height, std::min(caps.maxImageExtent.height, m_height));
        }
        u32 imageCount = caps.minImageCount + 1;
        if (caps.maxImageCount > 0 && imageCount > caps.maxImageCount) imageCount = caps.maxImageCount;

        VkSwapchainCreateInfoKHR info{};
        info.sType = VK_STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO_KHR;
        info.surface = m_surface;
        info.minImageCount = imageCount;
        info.imageFormat = chosen.format;
        info.imageColorSpace = chosen.colorSpace;
        info.imageExtent = extent;
        info.imageArrayLayers = 1;
        info.imageUsage = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT;
        info.imageSharingMode = VK_SHARING_MODE_EXCLUSIVE;
        info.preTransform = caps.currentTransform;
        info.compositeAlpha = VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR;
        info.presentMode = presentMode;
        info.clipped = VK_TRUE;
        info.oldSwapchain = m_swapchain;
        if (vkCreateSwapchainKHR(m_device, &info, nullptr, &m_swapchain) != VK_SUCCESS) return false;
        m_swapchainFormat = chosen.format;
        m_swapchainExtent = extent;

        u32 count = 0;
        vkGetSwapchainImagesKHR(m_device, m_swapchain, &count, nullptr);
        m_swapchainImages.resize(count);
        vkGetSwapchainImagesKHR(m_device, m_swapchain, &count, m_swapchainImages.data());
        m_swapchainViews.resize(count);
        for (u32 i = 0; i < count; ++i) {
            VkImageViewCreateInfo view{};
            view.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO;
            view.image = m_swapchainImages[i];
            view.viewType = VK_IMAGE_VIEW_TYPE_2D;
            view.format = m_swapchainFormat;
            view.subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
            if (vkCreateImageView(m_device, &view, nullptr, &m_swapchainViews[i]) != VK_SUCCESS) return false;
        }
        return true;
    }

    bool recreateSwapchain() {
        vkDeviceWaitIdle(m_device);
        destroySwapchain();
        if (!createSwapchain()) return false;
        if (!createDepthBuffer()) return false;
        m_frameSlot = 0;
        return true;
    }

    void destroySwapchain() {
        for (VkImageView view : m_swapchainViews) {
            if (view) vkDestroyImageView(m_device, view, nullptr);
        }
        m_swapchainViews.clear();
        m_swapchainImages.clear();
        destroyImage(m_depthImage);
        if (m_swapchain) {
            vkDestroySwapchainKHR(m_device, m_swapchain, nullptr);
            m_swapchain = VK_NULL_HANDLE;
        }
    }

    bool createCommands() {
        VkCommandPoolCreateInfo pool{};
        pool.sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO;
        pool.flags = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT;
        pool.queueFamilyIndex = m_graphicsFamily;
        if (vkCreateCommandPool(m_device, &pool, nullptr, &m_commandPool) != VK_SUCCESS) return false;
        m_commandBuffers.resize(kFramesInFlight);
        VkCommandBufferAllocateInfo alloc{};
        alloc.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO;
        alloc.commandPool = m_commandPool;
        alloc.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
        alloc.commandBufferCount = kFramesInFlight;
        if (vkAllocateCommandBuffers(m_device, &alloc, m_commandBuffers.data()) != VK_SUCCESS) return false;
        for (u32 i = 0; i < kFramesInFlight; ++i) {
            VkSemaphoreCreateInfo sem{};
            sem.sType = VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO;
            vkCreateSemaphore(m_device, &sem, nullptr, &m_imageAvailable[i]);
            vkCreateSemaphore(m_device, &sem, nullptr, &m_renderFinished[i]);
            VkFenceCreateInfo fence{};
            fence.sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO;
            fence.flags = VK_FENCE_CREATE_SIGNALED_BIT;
            vkCreateFence(m_device, &fence, nullptr, &m_inFlight[i]);
        }
        return true;
    }

    bool createFrameResources() {
        for (u32 i = 0; i < kFramesInFlight; ++i) {
            if (!createBuffer(sizeof(FrameData), VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT,
                              VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                              m_frameUniform[i], true)) return false;
            m_frameUniformMapped[i] = static_cast<u8*>(m_frameUniform[i].mapped);
        }
        if (!createBuffer(4 * 1024 * 1024, VK_BUFFER_USAGE_VERTEX_BUFFER_BIT,
                          VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT,
                          m_uiBuffer, true)) return false;
        m_uiMapped = static_cast<u8*>(m_uiBuffer.mapped);
        return true;
    }

    VkDescriptorSetLayout createSetLayout(std::initializer_list<VkDescriptorSetLayoutBinding> bindings) {
        VkDescriptorSetLayoutCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO;
        info.bindingCount = static_cast<u32>(bindings.size());
        info.pBindings = bindings.begin();
        VkDescriptorSetLayout layout = VK_NULL_HANDLE;
        vkCreateDescriptorSetLayout(m_device, &info, nullptr, &layout);
        return layout;
    }

    bool createDescriptorInfrastructure() {
        VkDescriptorSetLayoutBinding sceneBindings[3]{};
        sceneBindings[0].binding = 0;
        sceneBindings[0].descriptorType = VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER;
        sceneBindings[0].descriptorCount = 1;
        sceneBindings[0].stageFlags = VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT;
        sceneBindings[1].binding = 1;
        sceneBindings[1].descriptorType = VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE;
        sceneBindings[1].descriptorCount = 1;
        sceneBindings[1].stageFlags = VK_SHADER_STAGE_FRAGMENT_BIT;
        sceneBindings[2].binding = 2;
        sceneBindings[2].descriptorType = VK_DESCRIPTOR_TYPE_SAMPLER;
        sceneBindings[2].descriptorCount = 1;
        sceneBindings[2].stageFlags = VK_SHADER_STAGE_FRAGMENT_BIT;
        m_sceneSetLayout = createSetLayout({sceneBindings[0], sceneBindings[1], sceneBindings[2]});

        VkDescriptorSetLayoutBinding textureBindings[3]{};
        textureBindings[0].binding = 0;
        textureBindings[0].descriptorType = VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE;
        textureBindings[0].descriptorCount = 1;
        textureBindings[0].stageFlags = VK_SHADER_STAGE_FRAGMENT_BIT;
        textureBindings[1].binding = 1;
        textureBindings[1].descriptorType = VK_DESCRIPTOR_TYPE_SAMPLER;
        textureBindings[1].descriptorCount = 1;
        textureBindings[1].stageFlags = VK_SHADER_STAGE_FRAGMENT_BIT;
        textureBindings[2].binding = 2;
        textureBindings[2].descriptorType = VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE;
        textureBindings[2].descriptorCount = 1;
        textureBindings[2].stageFlags = VK_SHADER_STAGE_FRAGMENT_BIT;
        m_textureSetLayout = createSetLayout({textureBindings[0], textureBindings[1], textureBindings[2]});

        VkDescriptorSetLayoutBinding uiBindings[2]{};
        uiBindings[0] = textureBindings[0];
        uiBindings[1] = textureBindings[1];
        m_uiSetLayout = createSetLayout({uiBindings[0], uiBindings[1]});

        VkDescriptorPoolSize sizes[3]{};
        sizes[0].type = VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER;
        sizes[0].descriptorCount = kFramesInFlight;
        sizes[1].type = VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE;
        sizes[1].descriptorCount = kFramesInFlight + 3;
        sizes[2].type = VK_DESCRIPTOR_TYPE_SAMPLER;
        sizes[2].descriptorCount = kFramesInFlight + 2;
        VkDescriptorPoolCreateInfo pool{};
        pool.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO;
        pool.maxSets = kFramesInFlight + 2;
        pool.poolSizeCount = 3;
        pool.pPoolSizes = sizes;
        if (vkCreateDescriptorPool(m_device, &pool, nullptr, &m_descriptorPool) != VK_SUCCESS) return false;

        std::array<VkDescriptorSetLayout, kFramesInFlight> layouts{};
        for (auto& layout : layouts) layout = m_sceneSetLayout;
        VkDescriptorSetAllocateInfo alloc{};
        alloc.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO;
        alloc.descriptorPool = m_descriptorPool;
        alloc.descriptorSetCount = kFramesInFlight;
        alloc.pSetLayouts = layouts.data();
        if (vkAllocateDescriptorSets(m_device, &alloc, m_sceneSets.data()) != VK_SUCCESS) return false;

        VkDescriptorSetAllocateInfo textureAlloc{};
        textureAlloc.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO;
        textureAlloc.descriptorPool = m_descriptorPool;
        textureAlloc.descriptorSetCount = 1;
        textureAlloc.pSetLayouts = &m_textureSetLayout;
        if (vkAllocateDescriptorSets(m_device, &textureAlloc, &m_textureSet) != VK_SUCCESS) return false;

        VkDescriptorSetAllocateInfo uiAlloc{};
        uiAlloc.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO;
        uiAlloc.descriptorPool = m_descriptorPool;
        uiAlloc.descriptorSetCount = 1;
        uiAlloc.pSetLayouts = &m_uiSetLayout;
        if (vkAllocateDescriptorSets(m_device, &uiAlloc, &m_uiSet) != VK_SUCCESS) return false;

        for (u32 i = 0; i < kFramesInFlight; ++i) {
            VkDescriptorBufferInfo ubo{m_frameUniform[i].buffer, 0, sizeof(FrameData)};
            VkDescriptorImageInfo shadowInfo{};
            shadowInfo.imageLayout = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL;
            shadowInfo.imageView = m_shadowColor.view;
            VkDescriptorImageInfo shadowSamplerInfo{};
            shadowSamplerInfo.sampler = m_shadowSampler;
            VkWriteDescriptorSet writes[3]{};
            writes[0].sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET;
            writes[0].dstSet = m_sceneSets[i];
            writes[0].dstBinding = 0;
            writes[0].descriptorCount = 1;
            writes[0].descriptorType = VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER;
            writes[0].pBufferInfo = &ubo;
            writes[1].sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET;
            writes[1].dstSet = m_sceneSets[i];
            writes[1].dstBinding = 1;
            writes[1].descriptorCount = 1;
            writes[1].descriptorType = VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE;
            writes[1].pImageInfo = &shadowInfo;
            writes[2].sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET;
            writes[2].dstSet = m_sceneSets[i];
            writes[2].dstBinding = 2;
            writes[2].descriptorCount = 1;
            writes[2].descriptorType = VK_DESCRIPTOR_TYPE_SAMPLER;
            writes[2].pImageInfo = &shadowSamplerInfo;
            vkUpdateDescriptorSets(m_device, 3, writes, 0, nullptr);
        }
        return true;
    }

    VkShaderModule createShaderModule(const std::vector<u8>& code) {
        if (code.empty()) return VK_NULL_HANDLE;
        VkShaderModuleCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO;
        info.codeSize = code.size();
        info.pCode = reinterpret_cast<const u32*>(code.data());
        VkShaderModule module = VK_NULL_HANDLE;
        vkCreateShaderModule(m_device, &info, nullptr, &module);
        return module;
    }

    VkPipelineLayout createPipelineLayout(std::initializer_list<VkDescriptorSetLayout> sets,
                                          u32 pushConstantSize) {
        VkPushConstantRange range{};
        range.stageFlags = VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_FRAGMENT_BIT;
        range.offset = 0;
        range.size = pushConstantSize;
        VkPipelineLayoutCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO;
        info.setLayoutCount = static_cast<u32>(sets.size());
        info.pSetLayouts = sets.begin();
        info.pushConstantRangeCount = pushConstantSize ? 1 : 0;
        info.pPushConstantRanges = pushConstantSize ? &range : nullptr;
        VkPipelineLayout layout = VK_NULL_HANDLE;
        vkCreatePipelineLayout(m_device, &info, nullptr, &layout);
        return layout;
    }

    bool createScenePipeline(const std::vector<u8>& vs, const std::vector<u8>& ps, bool sky,
                             bool depthTest = true) {
        VkShaderModule vsModule = createShaderModule(vs);
        VkShaderModule psModule = createShaderModule(ps);
        if (!vsModule || !psModule) return false;
        VkPipelineShaderStageCreateInfo stages[2]{};
        stages[0].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
        stages[0].stage = VK_SHADER_STAGE_VERTEX_BIT;
        stages[0].module = vsModule;
        stages[0].pName = "VSMain";
        stages[1].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
        stages[1].stage = VK_SHADER_STAGE_FRAGMENT_BIT;
        stages[1].module = psModule;
        stages[1].pName = "PSMain";

        VkVertexInputBindingDescription binding{0, sizeof(Vertex), VK_VERTEX_INPUT_RATE_VERTEX};
        VkVertexInputAttributeDescription attrs[3]{};
        attrs[0] = {0, 0, VK_FORMAT_R32G32B32_SFLOAT, 0};
        attrs[1] = {1, 0, VK_FORMAT_R32G32B32_SFLOAT, 12};
        attrs[2] = {2, 0, VK_FORMAT_R32G32_SFLOAT, 24};
        VkPipelineVertexInputStateCreateInfo vertexInput{};
        vertexInput.sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO;
        vertexInput.vertexBindingDescriptionCount = sky ? 0 : 1;
        vertexInput.pVertexBindingDescriptions = sky ? nullptr : &binding;
        vertexInput.vertexAttributeDescriptionCount = sky ? 0 : 3;
        vertexInput.pVertexAttributeDescriptions = sky ? nullptr : attrs;

        VkPipelineInputAssemblyStateCreateInfo assembly{};
        assembly.sType = VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO;
        assembly.topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST;
        VkPipelineViewportStateCreateInfo viewport{};
        viewport.sType = VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO;
        viewport.viewportCount = 1;
        viewport.scissorCount = 1;
        VkPipelineRasterizationStateCreateInfo raster{};
        raster.sType = VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO;
        raster.polygonMode = VK_POLYGON_MODE_FILL;
        raster.cullMode = VK_CULL_MODE_NONE;
        raster.frontFace = VK_FRONT_FACE_COUNTER_CLOCKWISE;
        raster.lineWidth = 1.0f;
        VkPipelineMultisampleStateCreateInfo multisample{};
        multisample.sType = VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO;
        multisample.rasterizationSamples = VK_SAMPLE_COUNT_1_BIT;
        VkPipelineDepthStencilStateCreateInfo depth{};
        depth.sType = VK_STRUCTURE_TYPE_PIPELINE_DEPTH_STENCIL_STATE_CREATE_INFO;
        depth.depthTestEnable = sky ? VK_FALSE : VK_TRUE;
        depth.depthWriteEnable = sky ? VK_FALSE : VK_TRUE;
        depth.depthCompareOp = VK_COMPARE_OP_LESS_OR_EQUAL;
        VkPipelineColorBlendAttachmentState colorAttachment{};
        colorAttachment.blendEnable = VK_FALSE;
        colorAttachment.colorWriteMask = 0xF;
        VkPipelineColorBlendStateCreateInfo colorBlend{};
        colorBlend.sType = VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO;
        colorBlend.attachmentCount = 1;
        colorBlend.pAttachments = &colorAttachment;
        VkDynamicState dynamicStates[] = {VK_DYNAMIC_STATE_VIEWPORT, VK_DYNAMIC_STATE_SCISSOR};
        VkPipelineDynamicStateCreateInfo dynamic{};
        dynamic.sType = VK_STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO;
        dynamic.dynamicStateCount = 2;
        dynamic.pDynamicStates = dynamicStates;
        VkPipelineRenderingCreateInfo rendering{};
        rendering.sType = VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO;
        rendering.colorAttachmentCount = 1;
        rendering.pColorAttachmentFormats = &m_swapchainFormat;
        rendering.depthAttachmentFormat = VK_FORMAT_D32_SFLOAT;
        VkGraphicsPipelineCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO;
        info.pNext = &rendering;
        info.stageCount = 2;
        info.pStages = stages;
        info.pVertexInputState = &vertexInput;
        info.pInputAssemblyState = &assembly;
        info.pViewportState = &viewport;
        info.pRasterizationState = &raster;
        info.pMultisampleState = &multisample;
        info.pDepthStencilState = &depth;
        info.pColorBlendState = &colorBlend;
        info.pDynamicState = &dynamic;
        info.layout = sky ? m_skyPipelineLayout : m_scenePipelineLayout;
        VkPipeline* target = sky ? &m_skyPipeline : (depthTest ? &m_scenePipeline : &m_viewmodelPipeline);
        VkResult result = vkCreateGraphicsPipelines(m_device, VK_NULL_HANDLE, 1, &info, nullptr, target);
        if (result != VK_SUCCESS) log::error("Vulkan %s pipeline failed: %s", sky ? "sky" : "scene", vkResultName(result));
        vkDestroyShaderModule(m_device, vsModule, nullptr);
        vkDestroyShaderModule(m_device, psModule, nullptr);
        return result == VK_SUCCESS;
    }

    bool createUiPipeline(const std::vector<u8>& vs, const std::vector<u8>& ps) {
        VkShaderModule vsModule = createShaderModule(vs);
        VkShaderModule psModule = createShaderModule(ps);
        if (!vsModule || !psModule) return false;
        VkPipelineShaderStageCreateInfo stages[2]{};
        stages[0].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
        stages[0].stage = VK_SHADER_STAGE_VERTEX_BIT;
        stages[0].module = vsModule;
        stages[0].pName = "VSMain";
        stages[1].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
        stages[1].stage = VK_SHADER_STAGE_FRAGMENT_BIT;
        stages[1].module = psModule;
        stages[1].pName = "PSMain";

        VkVertexInputBindingDescription binding{0, sizeof(UIVertex), VK_VERTEX_INPUT_RATE_VERTEX};
        VkVertexInputAttributeDescription attrs[3]{};
        attrs[0] = {0, 0, VK_FORMAT_R32G32_SFLOAT, 0};
        attrs[1] = {1, 0, VK_FORMAT_R32G32_SFLOAT, 8};
        attrs[2] = {2, 0, VK_FORMAT_R32G32B32A32_SFLOAT, 16};
        VkPipelineVertexInputStateCreateInfo vertexInput{};
        vertexInput.sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO;
        vertexInput.vertexBindingDescriptionCount = 1;
        vertexInput.pVertexBindingDescriptions = &binding;
        vertexInput.vertexAttributeDescriptionCount = 3;
        vertexInput.pVertexAttributeDescriptions = attrs;
        VkPipelineInputAssemblyStateCreateInfo assembly{};
        assembly.sType = VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO;
        assembly.topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST;
        VkPipelineViewportStateCreateInfo viewport{};
        viewport.sType = VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO;
        viewport.viewportCount = 1;
        viewport.scissorCount = 1;
        VkPipelineRasterizationStateCreateInfo raster{};
        raster.sType = VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO;
        raster.polygonMode = VK_POLYGON_MODE_FILL;
        raster.cullMode = VK_CULL_MODE_NONE;
        raster.frontFace = VK_FRONT_FACE_COUNTER_CLOCKWISE;
        raster.lineWidth = 1.0f;
        VkPipelineMultisampleStateCreateInfo multisample{};
        multisample.sType = VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO;
        multisample.rasterizationSamples = VK_SAMPLE_COUNT_1_BIT;
        VkPipelineDepthStencilStateCreateInfo depth{};
        depth.sType = VK_STRUCTURE_TYPE_PIPELINE_DEPTH_STENCIL_STATE_CREATE_INFO;
        depth.depthTestEnable = VK_FALSE;
        depth.depthWriteEnable = VK_FALSE;
        depth.depthCompareOp = VK_COMPARE_OP_ALWAYS;
        VkPipelineColorBlendAttachmentState colorAttachment{};
        colorAttachment.blendEnable = VK_TRUE;
        colorAttachment.srcColorBlendFactor = VK_BLEND_FACTOR_SRC_ALPHA;
        colorAttachment.dstColorBlendFactor = VK_BLEND_FACTOR_ONE_MINUS_SRC_ALPHA;
        colorAttachment.colorBlendOp = VK_BLEND_OP_ADD;
        colorAttachment.srcAlphaBlendFactor = VK_BLEND_FACTOR_ONE;
        colorAttachment.dstAlphaBlendFactor = VK_BLEND_FACTOR_ONE_MINUS_SRC_ALPHA;
        colorAttachment.alphaBlendOp = VK_BLEND_OP_ADD;
        colorAttachment.colorWriteMask = 0xF;
        VkPipelineColorBlendStateCreateInfo colorBlend{};
        colorBlend.sType = VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO;
        colorBlend.attachmentCount = 1;
        colorBlend.pAttachments = &colorAttachment;
        VkDynamicState dynamicStates[] = {VK_DYNAMIC_STATE_VIEWPORT, VK_DYNAMIC_STATE_SCISSOR};
        VkPipelineDynamicStateCreateInfo dynamic{};
        dynamic.sType = VK_STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO;
        dynamic.dynamicStateCount = 2;
        dynamic.pDynamicStates = dynamicStates;
        VkPipelineRenderingCreateInfo rendering{};
        rendering.sType = VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO;
        rendering.colorAttachmentCount = 1;
        rendering.pColorAttachmentFormats = &m_swapchainFormat;
        rendering.depthAttachmentFormat = VK_FORMAT_D32_SFLOAT;
        VkGraphicsPipelineCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO;
        info.pNext = &rendering;
        info.stageCount = 2;
        info.pStages = stages;
        info.pVertexInputState = &vertexInput;
        info.pInputAssemblyState = &assembly;
        info.pViewportState = &viewport;
        info.pRasterizationState = &raster;
        info.pMultisampleState = &multisample;
        info.pDepthStencilState = &depth;
        info.pColorBlendState = &colorBlend;
        info.pDynamicState = &dynamic;
        info.layout = m_uiPipelineLayout;
        VkResult result = vkCreateGraphicsPipelines(m_device, VK_NULL_HANDLE, 1, &info, nullptr, &m_uiPipeline);
        vkDestroyShaderModule(m_device, vsModule, nullptr);
        vkDestroyShaderModule(m_device, psModule, nullptr);
        return result == VK_SUCCESS;
    }

    bool createPipelines() {
        auto sceneVS = readFile(Paths::asset("shaders/scene.vs.spv"));
        auto scenePS = readFile(Paths::asset("shaders/scene.ps.spv"));
        auto shadowVS = readFile(Paths::asset("shaders/shadow.vs.spv"));
        auto shadowPS = readFile(Paths::asset("shaders/shadow.ps.spv"));
        auto skyVS = readFile(Paths::asset("shaders/sky.vs.spv"));
        auto skyPS = readFile(Paths::asset("shaders/sky.ps.spv"));
        auto uiVS = readFile(Paths::asset("shaders/ui.vs.spv"));
        auto uiPS = readFile(Paths::asset("shaders/ui.ps.spv"));
        if (sceneVS.empty() || scenePS.empty() || shadowVS.empty() || shadowPS.empty() || skyVS.empty() || skyPS.empty() ||
            uiVS.empty() || uiPS.empty()) return false;

        m_scenePipelineLayout = createPipelineLayout({m_sceneSetLayout, m_textureSetLayout}, sizeof(PushData));
        m_skyPipelineLayout = createPipelineLayout({m_sceneSetLayout}, 0);
        m_shadowPipelineLayout = createPipelineLayout({}, sizeof(ShadowPush));
        m_uiPipelineLayout = createPipelineLayout({m_uiSetLayout}, sizeof(UIPush));
        if (!m_scenePipelineLayout || !m_skyPipelineLayout || !m_shadowPipelineLayout || !m_uiPipelineLayout) return false;

        if (!createScenePipeline(sceneVS, scenePS, false)) { log::error("Vulkan scene pipeline failed"); return false; }
        if (!createScenePipeline(sceneVS, scenePS, false, false)) { log::error("Vulkan viewmodel pipeline failed"); return false; }
        if (!createScenePipeline(skyVS, skyPS, true)) { log::error("Vulkan sky pipeline failed"); return false; }
        if (!createUiPipeline(uiVS, uiPS)) { log::error("Vulkan UI pipeline failed"); return false; }

        VkShaderModule shadowVSModule = createShaderModule(shadowVS);
        VkShaderModule shadowPSModule = createShaderModule(shadowPS);
        if (!shadowVSModule || !shadowPSModule) return false;
        VkPipelineShaderStageCreateInfo stages[2]{};
        stages[0].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
        stages[0].stage = VK_SHADER_STAGE_VERTEX_BIT;
        stages[0].module = shadowVSModule;
        stages[0].pName = "VSMain";
        stages[1].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
        stages[1].stage = VK_SHADER_STAGE_FRAGMENT_BIT;
        stages[1].module = shadowPSModule;
        stages[1].pName = "PSMain";
        VkVertexInputBindingDescription binding{0, sizeof(Vertex), VK_VERTEX_INPUT_RATE_VERTEX};
        VkVertexInputAttributeDescription attrs[3]{};
        attrs[0] = {0, 0, VK_FORMAT_R32G32B32_SFLOAT, 0};
        attrs[1] = {1, 0, VK_FORMAT_R32G32B32_SFLOAT, 12};
        attrs[2] = {2, 0, VK_FORMAT_R32G32_SFLOAT, 24};
        VkPipelineVertexInputStateCreateInfo vertexInput{};
        vertexInput.sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO;
        vertexInput.vertexBindingDescriptionCount = 1;
        vertexInput.pVertexBindingDescriptions = &binding;
        vertexInput.vertexAttributeDescriptionCount = 3;
        vertexInput.pVertexAttributeDescriptions = attrs;
        VkPipelineInputAssemblyStateCreateInfo assembly{};
        assembly.sType = VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO;
        assembly.topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST;
        VkPipelineViewportStateCreateInfo viewport{};
        viewport.sType = VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO;
        viewport.viewportCount = 1;
        viewport.scissorCount = 1;
        VkPipelineRasterizationStateCreateInfo raster{};
        raster.sType = VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO;
        raster.polygonMode = VK_POLYGON_MODE_FILL;
        raster.cullMode = VK_CULL_MODE_NONE;
        raster.frontFace = VK_FRONT_FACE_COUNTER_CLOCKWISE;
        raster.lineWidth = 1.0f;
        raster.depthBiasEnable = VK_TRUE;
        raster.depthBiasConstantFactor = 1.25f;
        raster.depthBiasSlopeFactor = 1.75f;
        VkPipelineMultisampleStateCreateInfo multisample{};
        multisample.sType = VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO;
        multisample.rasterizationSamples = VK_SAMPLE_COUNT_1_BIT;
        VkPipelineDepthStencilStateCreateInfo depth{};
        depth.sType = VK_STRUCTURE_TYPE_PIPELINE_DEPTH_STENCIL_STATE_CREATE_INFO;
        depth.depthTestEnable = VK_TRUE;
        depth.depthWriteEnable = VK_TRUE;
        depth.depthCompareOp = VK_COMPARE_OP_LESS_OR_EQUAL;
        VkPipelineColorBlendAttachmentState shadowAttachment{};
        shadowAttachment.blendEnable = VK_FALSE;
        shadowAttachment.colorWriteMask = 0xF;
        VkPipelineColorBlendStateCreateInfo colorBlend{};
        colorBlend.sType = VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO;
        colorBlend.attachmentCount = 1;
        colorBlend.pAttachments = &shadowAttachment;
        VkDynamicState dynamicStates[] = {VK_DYNAMIC_STATE_VIEWPORT, VK_DYNAMIC_STATE_SCISSOR};
        VkPipelineDynamicStateCreateInfo dynamic{};
        dynamic.sType = VK_STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO;
        dynamic.dynamicStateCount = 2;
        dynamic.pDynamicStates = dynamicStates;
        VkPipelineRenderingCreateInfo rendering{};
        rendering.sType = VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO;
        rendering.colorAttachmentCount = 1;
        rendering.pColorAttachmentFormats = &kShadowFormat;
        rendering.depthAttachmentFormat = VK_FORMAT_D32_SFLOAT;
        VkGraphicsPipelineCreateInfo info{};
        info.sType = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO;
        info.pNext = &rendering;
        info.stageCount = 2;
        info.pStages = stages;
        info.pVertexInputState = &vertexInput;
        info.pInputAssemblyState = &assembly;
        info.pViewportState = &viewport;
        info.pRasterizationState = &raster;
        info.pMultisampleState = &multisample;
        info.pDepthStencilState = &depth;
        info.pColorBlendState = &colorBlend;
        info.pDynamicState = &dynamic;
        info.layout = m_shadowPipelineLayout;
        VkResult result = vkCreateGraphicsPipelines(m_device, VK_NULL_HANDLE, 1, &info, nullptr, &m_shadowPipeline);
        if (result != VK_SUCCESS) log::error("Vulkan shadow pipeline failed: %s", vkResultName(result));
        vkDestroyShaderModule(m_device, shadowVSModule, nullptr);
        vkDestroyShaderModule(m_device, shadowPSModule, nullptr);
        return result == VK_SUCCESS;
    }

    VkCommandBuffer beginImmediate() {
        VkCommandBufferAllocateInfo alloc{};
        alloc.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO;
        alloc.commandPool = m_commandPool;
        alloc.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
        alloc.commandBufferCount = 1;
        VkCommandBuffer cmd = VK_NULL_HANDLE;
        if (vkAllocateCommandBuffers(m_device, &alloc, &cmd) != VK_SUCCESS) return VK_NULL_HANDLE;
        VkCommandBufferBeginInfo begin{};
        begin.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO;
        begin.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
        vkBeginCommandBuffer(cmd, &begin);
        return cmd;
    }

    void endImmediate(VkCommandBuffer cmd) {
        vkEndCommandBuffer(cmd);
        VkSubmitInfo submit{};
        submit.sType = VK_STRUCTURE_TYPE_SUBMIT_INFO;
        submit.commandBufferCount = 1;
        submit.pCommandBuffers = &cmd;
        vkQueueSubmit(m_graphicsQueue, 1, &submit, VK_NULL_HANDLE);
        vkQueueWaitIdle(m_graphicsQueue);
        vkFreeCommandBuffers(m_device, m_commandPool, 1, &cmd);
    }

    void transitionImage(VkCommandBuffer cmd, VkImage image, VkImageLayout oldLayout, VkImageLayout newLayout,
                         VkImageAspectFlags aspect, u32 baseMip, u32 baseLayer, VkPipelineStageFlags srcStage,
                         VkPipelineStageFlags dstStage, u32 layers) {
        VkImageMemoryBarrier barrier{};
        barrier.sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER;
        barrier.oldLayout = oldLayout;
        barrier.newLayout = newLayout;
        barrier.srcQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
        barrier.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
        barrier.image = image;
        barrier.subresourceRange = {aspect, baseMip, 1, baseLayer, layers};
        barrier.srcAccessMask = 0;
        barrier.dstAccessMask = 0;
        if (newLayout == VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL) barrier.dstAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
        else if (newLayout == VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL) barrier.dstAccessMask = VK_ACCESS_SHADER_READ_BIT;
        else if (newLayout == VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL) barrier.dstAccessMask = VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT;
        else if (newLayout == VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL) barrier.dstAccessMask = VK_ACCESS_DEPTH_STENCIL_ATTACHMENT_WRITE_BIT;
        if (oldLayout == VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL) barrier.srcAccessMask = VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT;
        else if (oldLayout == VK_IMAGE_LAYOUT_DEPTH_ATTACHMENT_OPTIMAL) barrier.srcAccessMask = VK_ACCESS_DEPTH_STENCIL_ATTACHMENT_WRITE_BIT;
        else if (oldLayout == VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL) barrier.srcAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
        else if (oldLayout == VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL) barrier.srcAccessMask = VK_ACCESS_SHADER_READ_BIT;
        vkCmdPipelineBarrier(cmd, srcStage, dstStage, 0, 0, nullptr, 0, nullptr, 1, &barrier);
    }

    VkInstance m_instance = VK_NULL_HANDLE;
    VkDebugUtilsMessengerEXT m_debugMessenger = VK_NULL_HANDLE;
    VkSurfaceKHR m_surface = VK_NULL_HANDLE;
    VkPhysicalDevice m_physicalDevice = VK_NULL_HANDLE;
    VkDevice m_device = VK_NULL_HANDLE;
    VkQueue m_graphicsQueue = VK_NULL_HANDLE;
    u32 m_graphicsFamily = 0;
    u32 m_apiVersion = 0;
    VkSwapchainKHR m_swapchain = VK_NULL_HANDLE;
    VkFormat m_swapchainFormat = VK_FORMAT_B8G8R8A8_UNORM;
    VkExtent2D m_swapchainExtent{};
    std::vector<VkImage> m_swapchainImages;
    std::vector<VkImageView> m_swapchainViews;
    ImageGPU m_depthImage;
    ImageGPU m_shadowImage;
    ImageGPU m_shadowColor;
    ImageGPU m_textureArray;
    ImageGPU m_font;
    ImageGPU m_viewTexture;
    VkSampler m_textureSampler = VK_NULL_HANDLE;
    VkSampler m_shadowSampler = VK_NULL_HANDLE;
    VkSampler m_fontSampler = VK_NULL_HANDLE;
    VkCommandPool m_commandPool = VK_NULL_HANDLE;
    std::vector<VkCommandBuffer> m_commandBuffers;
    std::array<VkSemaphore, kFramesInFlight> m_imageAvailable{};
    std::array<VkSemaphore, kFramesInFlight> m_renderFinished{};
    std::array<VkFence, kFramesInFlight> m_inFlight{};
    std::array<Buffer, kFramesInFlight> m_frameUniform;
    std::array<u8*, kFramesInFlight> m_frameUniformMapped{};
    Buffer m_uiBuffer;
    u8* m_uiMapped = nullptr;
    VkDescriptorSetLayout m_sceneSetLayout = VK_NULL_HANDLE;
    VkDescriptorSetLayout m_textureSetLayout = VK_NULL_HANDLE;
    VkDescriptorSetLayout m_uiSetLayout = VK_NULL_HANDLE;
    VkDescriptorPool m_descriptorPool = VK_NULL_HANDLE;
    std::array<VkDescriptorSet, kFramesInFlight> m_sceneSets{};
    VkDescriptorSet m_textureSet = VK_NULL_HANDLE;
    VkDescriptorSet m_uiSet = VK_NULL_HANDLE;
    VkPipelineLayout m_scenePipelineLayout = VK_NULL_HANDLE;
    VkPipelineLayout m_skyPipelineLayout = VK_NULL_HANDLE;
    VkPipelineLayout m_shadowPipelineLayout = VK_NULL_HANDLE;
    VkPipelineLayout m_uiPipelineLayout = VK_NULL_HANDLE;
    VkPipeline m_scenePipeline = VK_NULL_HANDLE;
    VkPipeline m_viewmodelPipeline = VK_NULL_HANDLE;
    VkPipeline m_shadowPipeline = VK_NULL_HANDLE;
    VkPipeline m_skyPipeline = VK_NULL_HANDLE;
    VkPipeline m_uiPipeline = VK_NULL_HANDLE;
    std::vector<MeshGPU> m_meshes;
    u32 m_frameSlot = 0;
    u32 m_width = 0;
    u32 m_height = 0;
    bool m_vsync = true;
    bool m_validation = false;
    bool m_swapchainDirty = false;
    bool m_loaderLoaded = false;
    int m_requestedAdapter = -1;
    std::string m_adapterName;
    Stats m_stats;
};

} // namespace

std::unique_ptr<Device> createVulkanDevice() {
    return std::make_unique<VulkanDevice>();
}

} // namespace vd::rhi

