#ifndef VOID_COMMON_HLSLI
#define VOID_COMMON_HLSLI

struct FrameData
{
    float4x4 viewProj;
    float4x4 invViewProj;
    float4x4 shadowMatrix;
    float4 cameraPos;
    float4 sunDirection;
    float4 sunColor;
    float4 ambient;
    float4 fog;
    float4 pointPos[3];
    float4 pointColor[3];
    float4 skyTop;
    float4 skyHorizon;
    float4 params; // x exposure, y time, z unused, w unused
};

struct PushData
{
    float4x4 model;
    float4 baseColor;
    float4 material; // x roughness, y metallic, z emissive, w texture layer
    float4 uvScale;
};

struct UIPush
{
    float4x4 ortho;
    float4 params;
};

static const float PI = 3.14159265359f;

float3 acesFilm(float3 x)
{
    const float a = 2.51f;
    const float b = 0.03f;
    const float c = 2.43f;
    const float d = 0.59f;
    const float e = 0.14f;
    return saturate((x * (a * x + b)) / (x * (c * x + d) + e));
}

float3 linearToSrgb(float3 c)
{
    return pow(saturate(c), 1.0f / 2.2f);
}

float3 finishColor(float3 linearColor, float exposure)
{
    return linearToSrgb(acesFilm(linearColor * exposure));
}

float distributionGGX(float nDotH, float roughness)
{
    float a = roughness * roughness;
    float a2 = a * a;
    float d = nDotH * nDotH * (a2 - 1.0f) + 1.0f;
    return a2 / max(PI * d * d, 1e-5f);
}

float geometrySchlick(float nDotV, float roughness)
{
    float r = roughness + 1.0f;
    float k = (r * r) / 8.0f;
    return nDotV / max(nDotV * (1.0f - k) + k, 1e-5f);
}

float3 fresnelSchlick(float cosTheta, float3 f0)
{
    return f0 + (1.0f - f0) * pow(saturate(1.0f - cosTheta), 5.0f);
}

#endif
