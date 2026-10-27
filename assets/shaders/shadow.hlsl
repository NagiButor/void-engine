struct ShadowPush
{
    float4x4 viewProj;
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

struct VSOutput
{
    float4 position : SV_Position;
};

VSOutput VSMain(VSInput input)
{
    VSOutput output;
    output.position = mul(gPush.viewProj, mul(gPush.model, float4(input.position, 1.0f)));
    return output;
}

float4 PSMain(VSOutput input) : SV_Target
{
    // R32_FLOAT shadow map stores the same normalized depth as a depth buffer,
    // but can be sampled as a normal texture on every driver.
    return float4(input.position.z, 0.0f, 0.0f, 1.0f);
}
