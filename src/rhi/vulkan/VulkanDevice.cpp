#include "void/rhi/RHI.h"

#include "void/core/Log.h"

#include <SDL3/SDL.h>
#include <SDL3/SDL_vulkan.h>
#include <volk.h>

#include <array>
#include <cstring>
#include <string>
#include <vector>

namespace vd::rhi {
namespace {

constexpr u32 kFramesInFlight = 2;

const char* vkResultName(VkResult result) {
    switch (result) {
    case VK_SUCCESS: return "VK_SUCCESS";
    case VK_NOT_READY: return "VK_NOT_READY";
    case VK_TIMEOUT: return "VK_TIMEOUT";
    case VK_EVENT_SET: return "VK_EVENT_SET";
    case VK_EVENT_RESET: return "VK_EVENT_RESET";
    case VK_INCOMPLETE: return "VK_INCOMPLETE";
    case VK_ERROR_OUT_OF_HOST_MEMORY: return "VK_ERROR_OUT_OF_HOST_MEMORY";
    case VK_ERROR_OUT_OF_DEVICE_MEMORY: return "VK_ERROR_OUT_OF_DEVICE_MEMORY";
    case VK_ERROR_INITIALIZATION_FAILED: return "VK_ERROR_INITIALIZATION_FAILED";
    case VK_ERROR_DEVICE_LOST: return "VK_ERROR_DEVICE_LOST";
    case VK_ERROR_MEMORY_MAP_FAILED: return "VK_ERROR_MEMORY_MAP_FAILED";
    case VK_ERROR_LAYER_NOT_PRESENT: return "VK_ERROR_LAYER_NOT_PRESENT";
    case VK_ERROR_EXTENSION_NOT_PRESENT: return "VK_ERROR_EXTENSION_NOT_PRESENT";
    case VK_ERROR_FEATURE_NOT_PRESENT: return "VK_ERROR_FEATURE_NOT_PRESENT";
    case VK_ERROR_INCOMPATIBLE_DRIVER: return "VK_ERROR_INCOMPATIBLE_DRIVER";
    case VK_ERROR_TOO_MANY_OBJECTS: return "VK_ERROR_TOO_MANY_OBJECTS";
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

        if (!createInstance(desc)) return false;
        if (!createSurface(desc.window, desc.adapter)) return false;
        if (!pickPhysicalDevice(desc)) return false;
        if (!createDevice()) return false;
        if (!createSwapchain()) return false;
        if (!createDepthBuffer()) return false;
        if (!createCommands()) return false;

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
            }
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

    MeshHandle createMesh(const MeshData&) override { return {}; }
    bool createTextureArray(const std::vector<ImageData>&) override { return false; }
    bool createFontTexture(const ImageData&) override { return false; }

    void render(const FrameData&, const DrawList&, const UIList&) override {
        if (!m_device) return;

        vkWaitForFences(m_device, 1, &m_inFlight[m_frameSlot], VK_TRUE, UINT64_MAX);

        u32 imageIndex = 0;
        VkResult acquire = vkAcquireNextImageKHR(
            m_device, m_swapchain, UINT64_MAX, m_imageAvailable[m_frameSlot], VK_NULL_HANDLE, &imageIndex);
        if (acquire == VK_ERROR_OUT_OF_DATE_KHR) {
            recreateSwapchain();
            return;
        }
        if (acquire != VK_SUCCESS && acquire != VK_SUBOPTIMAL_KHR) {
            log::error("vkAcquireNextImageKHR failed: %s", vkResultName(acquire));
            return;
        }
        if (acquire == VK_SUBOPTIMAL_KHR) m_swapchainDirty = true;

        vkResetFences(m_device, 1, &m_inFlight[m_frameSlot]);
        vkResetCommandBuffer(m_commandBuffers[m_frameSlot], 0);

        VkCommandBuffer cmd = m_commandBuffers[m_frameSlot];
        VkCommandBufferBeginInfo begin{};
        begin.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO;
        begin.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
        vkBeginCommandBuffer(cmd, &begin);

        VkImageMemoryBarrier toColor{};
        toColor.sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER;
        toColor.oldLayout = VK_IMAGE_LAYOUT_UNDEFINED;
        toColor.newLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL;
        toColor.srcQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
        toColor.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
        toColor.image = m_swapchainImages[imageIndex];
        toColor.subresourceRange.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
        toColor.subresourceRange.levelCount = 1;
        toColor.subresourceRange.layerCount = 1;
        toColor.srcAccessMask = 0;
        toColor.dstAccessMask = VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT;
        vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT,
                             VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
                             0, 0, nullptr, 0, nullptr, 1, &toColor);

        VkRenderingAttachmentInfo colorAttachment{};
        colorAttachment.sType = VK_STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO;
        colorAttachment.imageView = m_swapchainViews[imageIndex];
        colorAttachment.imageLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL;
        colorAttachment.loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR;
        colorAttachment.storeOp = VK_ATTACHMENT_STORE_OP_STORE;
        colorAttachment.clearValue.color = {{0.035f, 0.045f, 0.055f, 1.0f}};

        VkRenderingInfo rendering{};
        rendering.sType = VK_STRUCTURE_TYPE_RENDERING_INFO;
        rendering.renderArea = {{0, 0}, m_swapchainExtent};
        rendering.layerCount = 1;
        rendering.colorAttachmentCount = 1;
        rendering.pColorAttachments = &colorAttachment;

        vkCmdBeginRendering(cmd, &rendering);
        vkCmdEndRendering(cmd);

        VkImageMemoryBarrier toPresent = toColor;
        toPresent.oldLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL;
        toPresent.newLayout = VK_IMAGE_LAYOUT_PRESENT_SRC_KHR;
        toPresent.srcAccessMask = VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT;
        toPresent.dstAccessMask = 0;
        vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
                             VK_PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT,
                             0, 0, nullptr, 0, nullptr, 1, &toPresent);

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

        VkResult submitResult = vkQueueSubmit(m_graphicsQueue, 1, &submit, m_inFlight[m_frameSlot]);
        if (submitResult != VK_SUCCESS) {
            log::error("vkQueueSubmit failed: %s", vkResultName(submitResult));
            return;
        }

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
        } else if (presentResult != VK_SUCCESS) {
            log::error("vkQueuePresentKHR failed: %s", vkResultName(presentResult));
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
            if (m_validation) {
                extensions.push_back(VK_EXT_DEBUG_UTILS_EXTENSION_NAME);
            } else {
                log::warn("Vulkan validation layers requested but not installed");
            }
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
            debug.messageSeverity =
                VK_DEBUG_UTILS_MESSAGE_SEVERITY_WARNING_BIT_EXT |
                VK_DEBUG_UTILS_MESSAGE_SEVERITY_ERROR_BIT_EXT;
            debug.messageType =
                VK_DEBUG_UTILS_MESSAGE_TYPE_GENERAL_BIT_EXT |
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

    bool pickPhysicalDevice(const DeviceDesc&) {
        u32 count = 0;
        vkEnumeratePhysicalDevices(m_instance, &count, nullptr);
        if (count == 0) {
            log::error("Vulkan: no physical devices");
            return false;
        }
        std::vector<VkPhysicalDevice> devices(count);
        vkEnumeratePhysicalDevices(m_instance, &count, devices.data());

        VkPhysicalDevice best = VK_NULL_HANDLE;
        int bestScore = -1;
        for (u32 i = 0; i < count; ++i) {
            VkPhysicalDeviceProperties props{};
            vkGetPhysicalDeviceProperties(devices[i], &props);
            if (props.apiVersion < VK_API_VERSION_1_3) {
                log::warn("Vulkan: skipping %s (API %u.%u, need 1.3)", props.deviceName,
                          VK_API_VERSION_MAJOR(props.apiVersion), VK_API_VERSION_MINOR(props.apiVersion));
                continue;
            }

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
            bool hasSwapchain = false;
            for (const auto& ext : exts) {
                if (std::strcmp(ext.extensionName, VK_KHR_SWAPCHAIN_EXTENSION_NAME) == 0) hasSwapchain = true;
            }
            if (!hasSwapchain) continue;

            VkPhysicalDeviceVulkan13Features features{};
            features.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES;
            VkPhysicalDeviceFeatures2 features2{};
            features2.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2;
            features2.pNext = &features;
            vkGetPhysicalDeviceFeatures2(devices[i], &features2);
            if (!features.dynamicRendering) {
                log::warn("Vulkan: skipping %s (no dynamic rendering)", props.deviceName);
                continue;
            }

            int score = static_cast<int>(props.limits.maxImageDimension2D);
            if (props.deviceType == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU) score += 100000;
            if (props.deviceType == VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU) score += 50000;
            if (m_requestedAdapter >= 0 && static_cast<int>(i) != m_requestedAdapter) continue;

            if (score > bestScore) {
                bestScore = score;
                best = devices[i];
                m_graphicsFamily = static_cast<u32>(graphics);
                m_apiVersion = props.apiVersion;
                m_adapterName = props.deviceName;
            }
        }

        if (!best) {
            log::error("Vulkan: no suitable device (Vulkan 1.3 + dynamic rendering + swapchain)");
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

    u32 findMemoryType(u32 typeBits, VkMemoryPropertyFlags properties) const {
        VkPhysicalDeviceMemoryProperties mem{};
        vkGetPhysicalDeviceMemoryProperties(m_physicalDevice, &mem);
        for (u32 i = 0; i < mem.memoryTypeCount; ++i) {
            if ((typeBits & (1u << i)) && (mem.memoryTypes[i].propertyFlags & properties) == properties) {
                return i;
            }
        }
        return 0;
    }

    bool createDepthBuffer() {
        VkImageCreateInfo image{};
        image.sType = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO;
        image.imageType = VK_IMAGE_TYPE_2D;
        image.extent = {m_swapchainExtent.width, m_swapchainExtent.height, 1};
        image.mipLevels = 1;
        image.arrayLayers = 1;
        image.format = VK_FORMAT_D32_SFLOAT;
        image.tiling = VK_IMAGE_TILING_OPTIMAL;
        image.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED;
        image.usage = VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT | VK_IMAGE_USAGE_SAMPLED_BIT;
        image.samples = VK_SAMPLE_COUNT_1_BIT;
        image.sharingMode = VK_SHARING_MODE_EXCLUSIVE;

        VkResult result = vkCreateImage(m_device, &image, nullptr, &m_depthImage);
        if (result != VK_SUCCESS) return false;

        VkMemoryRequirements req{};
        vkGetImageMemoryRequirements(m_device, m_depthImage, &req);
        VkMemoryAllocateInfo alloc{};
        alloc.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO;
        alloc.allocationSize = req.size;
        alloc.memoryTypeIndex = findMemoryType(req.memoryTypeBits, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT);
        if (vkAllocateMemory(m_device, &alloc, nullptr, &m_depthMemory) != VK_SUCCESS) return false;
        vkBindImageMemory(m_device, m_depthImage, m_depthMemory, 0);

        VkImageViewCreateInfo view{};
        view.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO;
        view.image = m_depthImage;
        view.viewType = VK_IMAGE_VIEW_TYPE_2D;
        view.format = VK_FORMAT_D32_SFLOAT;
        view.subresourceRange.aspectMask = VK_IMAGE_ASPECT_DEPTH_BIT;
        view.subresourceRange.levelCount = 1;
        view.subresourceRange.layerCount = 1;
        return vkCreateImageView(m_device, &view, nullptr, &m_depthView) == VK_SUCCESS;
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
            if (format.format == VK_FORMAT_B8G8R8A8_UNORM &&
                format.colorSpace == VK_COLOR_SPACE_SRGB_NONLINEAR_KHR) {
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

        VkResult result = vkCreateSwapchainKHR(m_device, &info, nullptr, &m_swapchain);
        if (result != VK_SUCCESS) {
            log::error("vkCreateSwapchainKHR failed: %s", vkResultName(result));
            return false;
        }

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
            view.subresourceRange.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
            view.subresourceRange.levelCount = 1;
            view.subresourceRange.layerCount = 1;
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
        if (m_depthView) { vkDestroyImageView(m_device, m_depthView, nullptr); m_depthView = VK_NULL_HANDLE; }
        if (m_depthImage) { vkDestroyImage(m_device, m_depthImage, nullptr); m_depthImage = VK_NULL_HANDLE; }
        if (m_depthMemory) { vkFreeMemory(m_device, m_depthMemory, nullptr); m_depthMemory = VK_NULL_HANDLE; }
        for (VkImageView view : m_swapchainViews) {
            if (view) vkDestroyImageView(m_device, view, nullptr);
        }
        m_swapchainViews.clear();
        m_swapchainImages.clear();
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
    VkImage m_depthImage = VK_NULL_HANDLE;
    VkDeviceMemory m_depthMemory = VK_NULL_HANDLE;
    VkImageView m_depthView = VK_NULL_HANDLE;
    VkCommandPool m_commandPool = VK_NULL_HANDLE;
    std::vector<VkCommandBuffer> m_commandBuffers;
    std::array<VkSemaphore, kFramesInFlight> m_imageAvailable{};
    std::array<VkSemaphore, kFramesInFlight> m_renderFinished{};
    std::array<VkFence, kFramesInFlight> m_inFlight{};
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
