#include "common.hlsli"

#ifdef VOID_VULKAN
[[vk::binding(0, 0)]] cbuffer Frame : register(b0) { FrameData gFrame; };
#else
cbuffer Frame : register(b0) { FrameData gFrame; };
#endif

struct SkyOutput
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

SkyOutput VSMain(uint vertexId : SV_VertexID)
{
    SkyOutput output;
    float2 p;
    if (vertexId == 0) p = float2(-1.0f, -1.0f);
    else if (vertexId == 1) p = float2(3.0f, -1.0f);
    else p = float2(-1.0f, 3.0f);
    output.position = float4(p, 1.0f, 1.0f);
    output.uv = float2(p.x * 0.5f + 0.5f, 0.5f - p.y * 0.5f);
    return output;
}

float4 PSMain(SkyOutput input) : SV_Target
{
    float2 ndc = float2(input.uv.x * 2.0f - 1.0f, 1.0f - input.uv.y * 2.0f);
    float4 nearPoint = mul(gFrame.invViewProj, float4(ndc, 0.0f, 1.0f));
    float4 farPoint = mul(gFrame.invViewProj, float4(ndc, 1.0f, 1.0f));
    float3 worldNear = nearPoint.xyz / nearPoint.w;
    float3 worldFar = farPoint.xyz / farPoint.w;
    float3 ray = normalize(worldFar - worldNear);

    float height = saturate(ray.y);
    float3 sky = lerp(gFrame.skyHorizon.rgb, gFrame.skyTop.rgb, pow(height, 0.65f));

    float3 sunDir = normalize(gFrame.sunDirection.xyz);
    float sunDot = saturate(dot(ray, sunDir));
    float sun = pow(sunDot, 350.0f);
    float glow = pow(sunDot, 12.0f) * 0.18f;
    sky += gFrame.sunColor.rgb * (sun * 1.8f + glow) * gFrame.sunColor.w;

    return float4(finishColor(sky, gFrame.params.x), 1.0f);
}
