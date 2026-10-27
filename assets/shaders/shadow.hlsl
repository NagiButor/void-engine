struct ShadowPush
{
    float4x4 model;
};

#ifdef VOID_VULKAN
[[vk::push_constant]] ConstantBuffer<ShadowPush> gPush;
#else
cbuffer ShadowConstants : register(b0) { ShadowPush gPush; };
#endif

struct VSInput
{
    float3 position : POSITION;
    float3 normal : NORMAL;
    float2 uv : TEXCOORD0;
};

float4 VSMain(VSInput input) : SV_Position
{
    return mul(gPush.model, float4(input.position, 1.0f));
}
