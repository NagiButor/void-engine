#include "common.hlsli"

#ifdef VOID_VULKAN
[[vk::binding(0, 0)]] Texture2D gFont : register(t0);
[[vk::binding(1, 0)]] SamplerState gFontSampler : register(s0);
[[vk::push_constant]] ConstantBuffer<UIPush> gUI;
#else
Texture2D gFont : register(t0);
SamplerState gFontSampler : register(s0);
cbuffer UIConstants : register(b0) { UIPush gUI; };
#endif

struct UIInput
{
    float2 position : POSITION;
    float2 uv : TEXCOORD0;
    float4 color : COLOR0;
};

struct UIOutput
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
    float4 color : COLOR0;
};

UIOutput VSMain(UIInput input)
{
    UIOutput output;
    output.position = mul(gUI.ortho, float4(input.position, 0.0f, 1.0f));
    output.uv = input.uv;
    output.color = input.color;
    return output;
}

float4 PSMain(UIOutput input) : SV_Target
{
    float alpha = gFont.Sample(gFontSampler, input.uv).r;
    return float4(input.color.rgb, input.color.a * alpha);
}
