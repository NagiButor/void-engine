#include "common.hlsli"

#ifdef VOID_VULKAN
[[vk::binding(0, 0)]] cbuffer Frame : register(b0) { FrameData gFrame; };
[[vk::binding(1, 0)]] Texture2D gShadowMap : register(t0);
[[vk::binding(2, 0)]] SamplerState gShadowSampler : register(s0);
[[vk::binding(0, 1)]] Texture2DArray gTextures : register(t1);
[[vk::binding(1, 1)]] SamplerState gLinearSampler : register(s1);
[[vk::binding(2, 1)]] Texture2D gViewTexture : register(t2);
[[vk::push_constant]] ConstantBuffer<PushData> gPush;
#else
cbuffer Frame : register(b0) { FrameData gFrame; };
Texture2D gShadowMap : register(t0);
SamplerState gShadowSampler : register(s0);
Texture2DArray gTextures : register(t1);
SamplerState gLinearSampler : register(s1);
Texture2D gViewTexture : register(t2);
cbuffer Push : register(b2) { PushData gPush; };
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
    float3 worldPos : TEXCOORD0;
    float3 normal : TEXCOORD1;
    float2 uv : TEXCOORD2;
};

VSOutput VSMain(VSInput input)
{
    VSOutput output;
    float4 world = mul(gPush.model, float4(input.position, 1.0f));
    output.worldPos = world.xyz;
    output.normal = normalize(mul((float3x3)gPush.model, input.normal));
    output.uv = input.uv * gPush.uvScale.xy;
    output.position = mul(gFrame.viewProj, world);
    return output;
}

float shadowFactor(float3 worldPos, float nDotL)
{
    float4 shadowPos = mul(gFrame.shadowMatrix, float4(worldPos, 1.0f));
    float3 projected = shadowPos.xyz / max(shadowPos.w, 1e-5f);
    if (projected.x < 0.0f || projected.x > 1.0f ||
        projected.y < 0.0f || projected.y > 1.0f ||
        projected.z < 0.0f || projected.z > 1.0f)
    {
        return 1.0f;
    }

    float bias = max(0.0025f * (1.0f - nDotL), 0.0006f);
    float2 texel = float2(1.0f / 2048.0f, 1.0f / 2048.0f);
    float result = 0.0f;
    [unroll]
    for (int y = -1; y <= 1; ++y)
    {
        [unroll]
        for (int x = -1; x <= 1; ++x)
        {
            float depth = gShadowMap.SampleLevel(gShadowSampler, projected.xy + float2(x, y) * texel, 0).r;
            result += (projected.z - bias) <= depth ? 1.0f : 0.0f;
        }
    }
    return result / 9.0f;
}

float3 shadePoint(float3 albedo, float3 N, float3 V, float3 L, float3 radiance,
                  float3 f0, float roughness, float metallic)
{
    float3 H = normalize(L + V);
    float nDotL = saturate(dot(N, L));
    float nDotV = saturate(dot(N, V));
    float nDotH = saturate(dot(N, H));
    float vDotH = saturate(dot(V, H));

    float D = distributionGGX(nDotH, roughness);
    float G = geometrySchlick(nDotV, roughness) * geometrySchlick(nDotL, roughness);
    float3 F = fresnelSchlick(vDotH, f0);
    float3 specular = (D * G * F) / max(4.0f * nDotV * nDotL, 1e-4f);
    float3 diffuse = (1.0f - F) * (1.0f - metallic) * albedo / PI;
    return (diffuse + specular) * radiance * nDotL;
}

float4 PSMain(VSOutput input) : SV_Target
{
    float3 N = normalize(input.normal);
    float3 V = normalize(gFrame.cameraPos.xyz - input.worldPos);
    float3 albedo;
    if (gPush.material.w > 200.0f) {
        albedo = gViewTexture.Sample(gLinearSampler, input.uv).rgb * gPush.baseColor.rgb;
    } else {
        albedo = gTextures.Sample(gLinearSampler, float3(input.uv, gPush.material.w)).rgb * gPush.baseColor.rgb;
    }
    // Baked viewmodel textures already contain lighting; show them as authored.
    if (gPush.uvScale.z > 0.5f) {
        return float4(linearToSrgb(albedo), 1.0f);
    }
    float roughness = clamp(gPush.material.x, 0.045f, 1.0f);
    float metallic = saturate(gPush.material.y);
    float emissive = gPush.material.z;
    float3 f0 = lerp(0.04f.xxx, albedo, metallic);

    float3 L = normalize(gFrame.sunDirection.xyz);
    float nDotL = saturate(dot(N, L));
    float shadow = shadowFactor(input.worldPos, nDotL);
    float3 sunRadiance = gFrame.sunColor.rgb * gFrame.sunColor.w;
    float3 color = shadePoint(albedo, N, V, L, sunRadiance * shadow, f0, roughness, metallic);

    [unroll]
    for (int i = 0; i < 3; ++i)
    {
        float3 toLight = gFrame.pointPos[i].xyz - input.worldPos;
        float distanceSq = max(dot(toLight, toLight), 1e-4f);
        float range = max(gFrame.pointPos[i].w, 0.1f);
        float attenuation = saturate(1.0f - distanceSq / (range * range));
        attenuation *= attenuation;
        float3 pointDir = normalize(toLight);
        float3 radiance = gFrame.pointColor[i].rgb * gFrame.pointColor[i].w * attenuation / distanceSq;
        color += shadePoint(albedo, N, V, pointDir, radiance, f0, roughness, metallic);
    }

    float3 ambient = albedo * gFrame.ambient.rgb * gFrame.ambient.w;
    color += ambient;
    color += albedo * emissive * 2.0f;

    float distanceToCamera = length(gFrame.cameraPos.xyz - input.worldPos);
    float fogAmount = 1.0f - exp(-gFrame.fog.w * distanceToCamera);
    color = lerp(color, gFrame.fog.rgb, saturate(fogAmount));

    return float4(finishColor(color, gFrame.params.x), 1.0f);
}
