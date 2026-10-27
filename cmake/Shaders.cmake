# Shader compilation. DXC ships with the Windows 10/11 SDK and can emit
# both DXIL (DX12) and SPIR-V (Vulkan) from the same HLSL sources.
# The dxc.exe bundled with the Windows SDK is built without SPIR-V codegen.
# Prefer a standalone DXC release (downloaded into the build tree), then a
# user-provided tool, then the SDK.
set(VOID_DXC_LOCAL "${CMAKE_CURRENT_SOURCE_DIR}/tools/dxc/bin/x64/dxc.exe")
if(EXISTS "${VOID_DXC_LOCAL}")
    set(VOID_DXC_EXECUTABLE "${VOID_DXC_LOCAL}" CACHE FILEPATH "" FORCE)
elseif(NOT VOID_DXC_EXECUTABLE)
    FetchContent_Declare(dxc
        URL https://github.com/microsoft/DirectXShaderCompiler/releases/download/v1.9.2609/dxc_2026_09_29.zip
        DOWNLOAD_EXTRACT_TIMESTAMP TRUE
    )
    FetchContent_MakeAvailable(dxc)
    set(VOID_DXC_EXECUTABLE "${dxc_SOURCE_DIR}/bin/x64/dxc.exe" CACHE FILEPATH "" FORCE)
endif()
if(NOT EXISTS "${VOID_DXC_EXECUTABLE}")
    message(FATAL_ERROR "DXC (dxc.exe) was not found. Install a standalone DXC or pass -DVOID_DXC_EXECUTABLE=...")
endif()
message(STATUS "V.O.I.D. shader compiler: ${VOID_DXC_EXECUTABLE}")

set(VOID_SHADER_SRC "${CMAKE_CURRENT_SOURCE_DIR}/assets/shaders")
set(VOID_SHADER_OUT "${CMAKE_BINARY_DIR}/assets/shaders")
file(MAKE_DIRECTORY "${VOID_SHADER_OUT}")

set_property(GLOBAL PROPERTY VOID_SHADER_OUTPUTS "")

function(void_compile_shader FILE ENTRY PROFILE OUTNAME IS_SPIRV)
    set(OUT "${VOID_SHADER_OUT}/${OUTNAME}")
    set(EXTRA)
    if(IS_SPIRV)
        list(APPEND EXTRA -spirv -DVOID_VULKAN=1)
    endif()
    add_custom_command(
        OUTPUT "${OUT}"
        COMMAND "${VOID_DXC_EXECUTABLE}" -nologo
                -T ${PROFILE} -E ${ENTRY}
                ${EXTRA}
                -I "${VOID_SHADER_SRC}"
                -Fo "${OUT}"
                "${VOID_SHADER_SRC}/${FILE}"
        DEPENDS "${VOID_SHADER_SRC}/${FILE}"
                "${VOID_SHADER_SRC}/common.hlsli"
        COMMENT "DXC ${FILE} ${ENTRY} -> ${OUTNAME}"
        VERBATIM
    )
    set_property(GLOBAL APPEND PROPERTY VOID_SHADER_OUTPUTS "${OUT}")
endfunction()

void_compile_shader(scene.hlsl VSMain vs_6_0 scene.vs.dxil FALSE)
void_compile_shader(scene.hlsl PSMain ps_6_0 scene.ps.dxil FALSE)
void_compile_shader(scene.hlsl VSMain vs_6_0 scene.vs.spv TRUE)
void_compile_shader(scene.hlsl PSMain ps_6_0 scene.ps.spv TRUE)
void_compile_shader(shadow.hlsl VSMain vs_6_0 shadow.vs.dxil FALSE)
void_compile_shader(shadow.hlsl VSMain vs_6_0 shadow.vs.spv TRUE)
void_compile_shader(sky.hlsl VSMain vs_6_0 sky.vs.dxil FALSE)
void_compile_shader(sky.hlsl PSMain ps_6_0 sky.ps.dxil FALSE)
void_compile_shader(sky.hlsl VSMain vs_6_0 sky.vs.spv TRUE)
void_compile_shader(sky.hlsl PSMain ps_6_0 sky.ps.spv TRUE)
void_compile_shader(ui.hlsl VSMain vs_6_0 ui.vs.dxil FALSE)
void_compile_shader(ui.hlsl PSMain ps_6_0 ui.ps.dxil FALSE)
void_compile_shader(ui.hlsl VSMain vs_6_0 ui.vs.spv TRUE)
void_compile_shader(ui.hlsl PSMain ps_6_0 ui.ps.spv TRUE)

get_property(VOID_SHADER_OUTPUTS GLOBAL PROPERTY VOID_SHADER_OUTPUTS)
add_custom_target(void_shaders ALL DEPENDS ${VOID_SHADER_OUTPUTS})
