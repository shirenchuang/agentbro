use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPreset {
    pub id: String,
    pub name: String,
    pub category: String,
    pub icon: String,
    pub icon_color: String,
    pub website_url: String,
    pub settings_template: Value,
    pub description: String,
    pub supported_apps: Vec<String>,
}

fn apps_all() -> Vec<String> {
    vec!["claude".into(), "codex".into(), "gemini".into(), "opencode".into(), "hermes".into()]
}

fn apps_claude() -> Vec<String> {
    vec!["claude".into()]
}

fn apps_claude_opencode_hermes() -> Vec<String> {
    vec!["claude".into(), "opencode".into(), "hermes".into()]
}

fn apps_claude_opencode() -> Vec<String> {
    vec!["claude".into(), "opencode".into()]
}

fn apps_claude_gemini() -> Vec<String> {
    vec!["claude".into(), "gemini".into()]
}

fn apps_claude_opencode_gemini_hermes() -> Vec<String> {
    vec!["claude".into(), "opencode".into(), "gemini".into(), "hermes".into()]
}

pub fn list_presets() -> Vec<ProviderPreset> {
    vec![
        // ========================
        // Official
        // ========================
        ProviderPreset {
            id: "claude-official".into(),
            name: "Claude Official".into(),
            category: "official".into(),
            icon: "anthropic".into(),
            icon_color: "#D4915D".into(),
            website_url: "https://www.anthropic.com/claude-code".into(),
            settings_template: json!({ "env": {} }),
            description: "Anthropic 官方 API".into(),
            supported_apps: apps_claude(),
        },
        ProviderPreset {
            id: "google-official".into(),
            name: "Google Official".into(),
            category: "official".into(),
            icon: "gemini".into(),
            icon_color: "#4285F4".into(),
            website_url: "https://ai.google.dev/".into(),
            settings_template: json!({ "env": {} }),
            description: "Google 官方 Gemini API".into(),
            supported_apps: vec!["gemini".into()],
        },
        ProviderPreset {
            id: "openai-direct".into(),
            name: "OpenAI Direct".into(),
            category: "official".into(),
            icon: "openai".into(),
            icon_color: "#10A37F".into(),
            website_url: "https://platform.openai.com".into(),
            settings_template: json!({
                "primaryApiKey": "",
                "baseUrl": "https://api.openai.com/v1"
            }),
            description: "OpenAI 官方 API".into(),
            supported_apps: vec!["codex".into()],
        },
        // ========================
        // Chinese AI (cn_official)
        // ========================
        ProviderPreset {
            id: "volcengine-agentplan".into(),
            name: "火山 Agentplan".into(),
            category: "cn_official".into(),
            icon: "huoshan".into(),
            icon_color: "#3370FF".into(),
            website_url: "https://www.volcengine.com/activity/agentplan".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://ark.cn-beijing.volces.com/api/coding",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "ark-code-latest",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "ark-code-latest",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "ark-code-latest",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "ark-code-latest"
                }
            }),
            description: "火山引擎 Agentplan 编程方案".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "byteplus".into(),
            name: "BytePlus".into(),
            category: "cn_official".into(),
            icon: "byteplus".into(),
            icon_color: "#3370FF".into(),
            website_url: "https://www.byteplus.com/en/product/modelark".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://ark.ap-southeast.bytepluses.com/api/coding",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "ark-code-latest",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "ark-code-latest",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "ark-code-latest",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "ark-code-latest"
                }
            }),
            description: "BytePlus ModelArk (海外版火山引擎)".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "doubaoseed".into(),
            name: "DouBaoSeed".into(),
            category: "cn_official".into(),
            icon: "doubao".into(),
            icon_color: "#3370FF".into(),
            website_url: "https://console.volcengine.com/ark".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://ark.cn-beijing.volces.com/api/compatible",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "API_TIMEOUT_MS": "3000000",
                    "ANTHROPIC_MODEL": "doubao-seed-2-0-code-preview-latest",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "doubao-seed-2-0-code-preview-latest",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "doubao-seed-2-0-code-preview-latest",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "doubao-seed-2-0-code-preview-latest"
                }
            }),
            description: "字节跳动豆包 Seed 编程模型".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            category: "cn_official".into(),
            icon: "deepseek".into(),
            icon_color: "#1E88E5".into(),
            website_url: "https://platform.deepseek.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "deepseek-v4-pro",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "deepseek-v4-flash",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "deepseek-v4-pro",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "deepseek-v4-pro"
                }
            }),
            description: "DeepSeek V4 Pro/Flash".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "zhipu-glm".into(),
            name: "Zhipu GLM".into(),
            category: "cn_official".into(),
            icon: "zhipu".into(),
            icon_color: "#0F62FE".into(),
            website_url: "https://open.bigmodel.cn".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://open.bigmodel.cn/api/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "glm-5",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "glm-5",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-5",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "glm-5"
                }
            }),
            description: "智谱 GLM-5 模型".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "zhipu-glm-en".into(),
            name: "Zhipu GLM en".into(),
            category: "cn_official".into(),
            icon: "zhipu".into(),
            icon_color: "#0F62FE".into(),
            website_url: "https://z.ai".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.z.ai/api/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "glm-5",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "glm-5",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-5",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "glm-5"
                }
            }),
            description: "智谱 GLM-5 国际版".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "baidu-qianfan".into(),
            name: "Baidu Qianfan Coding".into(),
            category: "cn_official".into(),
            icon: "baidu".into(),
            icon_color: "#2932E1".into(),
            website_url: "https://cloud.baidu.com/product/qianfan_modelbuilder".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://qianfan.baidubce.com/anthropic/coding",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "qianfan-code-latest",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "qianfan-code-latest",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "qianfan-code-latest",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "qianfan-code-latest"
                }
            }),
            description: "百度千帆 Coding Plan".into(),
            supported_apps: apps_claude(),
        },
        ProviderPreset {
            id: "bailian".into(),
            name: "Bailian".into(),
            category: "cn_official".into(),
            icon: "bailian".into(),
            icon_color: "#624AFF".into(),
            website_url: "https://bailian.console.aliyun.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://dashscope.aliyuncs.com/apps/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "阿里百炼 (通义千问)".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "bailian-coding".into(),
            name: "Bailian For Coding".into(),
            category: "cn_official".into(),
            icon: "bailian".into(),
            icon_color: "#624AFF".into(),
            website_url: "https://bailian.console.aliyun.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://coding.dashscope.aliyuncs.com/apps/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "阿里百炼 Coding 专用".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "kimi".into(),
            name: "Kimi".into(),
            category: "cn_official".into(),
            icon: "kimi".into(),
            icon_color: "#6366F1".into(),
            website_url: "https://platform.moonshot.cn/console".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.moonshot.cn/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "kimi-k2.6",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "kimi-k2.6",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "kimi-k2.6",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "kimi-k2.6"
                }
            }),
            description: "月之暗面 Kimi K2.6".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "kimi-coding".into(),
            name: "Kimi For Coding".into(),
            category: "cn_official".into(),
            icon: "kimi".into(),
            icon_color: "#6366F1".into(),
            website_url: "https://www.kimi.com/code/docs/".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "Kimi Coding 专用方案".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "stepfun".into(),
            name: "StepFun".into(),
            category: "cn_official".into(),
            icon: "stepfun".into(),
            icon_color: "#16D6D2".into(),
            website_url: "https://platform.stepfun.com/step-plan".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.stepfun.com/step_plan",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "step-3.5-flash-2603",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "step-3.5-flash-2603",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "step-3.5-flash-2603",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "step-3.5-flash-2603"
                }
            }),
            description: "阶跃星辰 Step Plan".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "stepfun-en".into(),
            name: "StepFun en".into(),
            category: "cn_official".into(),
            icon: "stepfun".into(),
            icon_color: "#16D6D2".into(),
            website_url: "https://platform.stepfun.ai/step-plan".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.stepfun.ai/step_plan",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "step-3.5-flash-2603",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "step-3.5-flash-2603",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "step-3.5-flash-2603",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "step-3.5-flash-2603"
                }
            }),
            description: "StepFun 国际版".into(),
            supported_apps: apps_claude_opencode(),
        },
        ProviderPreset {
            id: "minimax".into(),
            name: "MiniMax".into(),
            category: "cn_official".into(),
            icon: "minimax".into(),
            icon_color: "#FF6B6B".into(),
            website_url: "https://platform.minimaxi.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.minimaxi.com/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "API_TIMEOUT_MS": "3000000",
                    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": 1,
                    "ANTHROPIC_MODEL": "MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "MiniMax-M2.7"
                }
            }),
            description: "MiniMax M2.7 模型".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "minimax-en".into(),
            name: "MiniMax en".into(),
            category: "cn_official".into(),
            icon: "minimax".into(),
            icon_color: "#FF6B6B".into(),
            website_url: "https://platform.minimax.io".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.minimax.io/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "API_TIMEOUT_MS": "3000000",
                    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": 1,
                    "ANTHROPIC_MODEL": "MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "MiniMax-M2.7"
                }
            }),
            description: "MiniMax 国际版".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "bailing".into(),
            name: "BaiLing".into(),
            category: "cn_official".into(),
            icon: "bailing".into(),
            icon_color: "#624AFF".into(),
            website_url: "https://alipaytbox.yuque.com/sxs0ba/ling/get_started".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.tbox.cn/api/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "Ling-2.5-1T",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "Ling-2.5-1T",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "Ling-2.5-1T",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "Ling-2.5-1T"
                }
            }),
            description: "蚂蚁百灵 Ling 2.5".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "kat-coder".into(),
            name: "KAT-Coder".into(),
            category: "cn_official".into(),
            icon: "catcoder".into(),
            icon_color: "#6366F1".into(),
            website_url: "https://console.streamlake.ai".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://vanchin.streamlake.ai/api/gateway/v1/endpoints/${ENDPOINT_ID}/claude-code-proxy",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "KAT-Coder-Pro V1",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "KAT-Coder-Air V1",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "KAT-Coder-Pro V1",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "KAT-Coder-Pro V1"
                }
            }),
            description: "快手 KAT-Coder 编程模型".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "longcat".into(),
            name: "Longcat".into(),
            category: "cn_official".into(),
            icon: "longcat".into(),
            icon_color: "#29E154".into(),
            website_url: "https://longcat.chat/platform".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.longcat.chat/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "LongCat-Flash-Chat",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "LongCat-Flash-Chat",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "LongCat-Flash-Chat",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "LongCat-Flash-Chat",
                    "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "6000",
                    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": 1
                }
            }),
            description: "Longcat Flash 长文本模型".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "xiaomi-mimo".into(),
            name: "Xiaomi MiMo".into(),
            category: "cn_official".into(),
            icon: "xiaomimimo".into(),
            icon_color: "#000000".into(),
            website_url: "https://platform.xiaomimimo.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.xiaomimimo.com/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "mimo-v2.5-pro",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "mimo-v2.5-pro",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "mimo-v2.5-pro",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "mimo-v2.5-pro"
                }
            }),
            description: "小米 MiMo v2.5 Pro".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "xiaomi-mimo-token".into(),
            name: "Xiaomi MiMo Token Plan".into(),
            category: "cn_official".into(),
            icon: "xiaomimimo".into(),
            icon_color: "#000000".into(),
            website_url: "https://platform.xiaomimimo.com/#/token-plan".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://token-plan-cn.xiaomimimo.com/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "mimo-v2.5-pro",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "mimo-v2.5-pro",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "mimo-v2.5-pro",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "mimo-v2.5-pro"
                }
            }),
            description: "小米 MiMo Token Plan (国内)".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "gemini-native".into(),
            name: "Gemini Native".into(),
            category: "cn_official".into(),
            icon: "gemini".into(),
            icon_color: "#4285F4".into(),
            website_url: "https://ai.google.dev/gemini-api".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://generativelanguage.googleapis.com",
                    "ANTHROPIC_API_KEY": "",
                    "ANTHROPIC_MODEL": "gemini-3.1-pro",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "gemini-3-flash",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "gemini-3.1-pro",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "gemini-3.1-pro"
                }
            }),
            description: "Gemini Native API (Claude Code 格式转换)".into(),
            supported_apps: apps_claude(),
        },
        // ========================
        // Aggregator
        // ========================
        ProviderPreset {
            id: "shengsuanyun".into(),
            name: "胜算云".into(),
            category: "aggregator".into(),
            icon: "shengsuanyun".into(),
            icon_color: "#00A67E".into(),
            website_url: "https://www.shengsuanyun.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://router.shengsuanyun.com/api",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "胜算云 API 聚合".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "modelscope".into(),
            name: "ModelScope".into(),
            category: "aggregator".into(),
            icon: "modelscope".into(),
            icon_color: "#624AFF".into(),
            website_url: "https://modelscope.cn".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api-inference.modelscope.cn",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "ZhipuAI/GLM-5",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "ZhipuAI/GLM-5",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "ZhipuAI/GLM-5",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "ZhipuAI/GLM-5"
                }
            }),
            description: "魔搭社区模型推理".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "aihubmix".into(),
            name: "AiHubMix".into(),
            category: "aggregator".into(),
            icon: "aihubmix".into(),
            icon_color: "#006FFB".into(),
            website_url: "https://aihubmix.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://aihubmix.com",
                    "ANTHROPIC_API_KEY": ""
                }
            }),
            description: "AiHubMix 多模型聚合".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "siliconflow".into(),
            name: "SiliconFlow".into(),
            category: "aggregator".into(),
            icon: "siliconflow".into(),
            icon_color: "#6E29F6".into(),
            website_url: "https://siliconflow.cn".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.siliconflow.cn",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "Pro/MiniMaxAI/MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "Pro/MiniMaxAI/MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "Pro/MiniMaxAI/MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "Pro/MiniMaxAI/MiniMax-M2.7"
                }
            }),
            description: "硅基流动 SiliconFlow".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "siliconflow-en".into(),
            name: "SiliconFlow en".into(),
            category: "aggregator".into(),
            icon: "siliconflow".into(),
            icon_color: "#000000".into(),
            website_url: "https://siliconflow.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.siliconflow.com",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "MiniMaxAI/MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "MiniMaxAI/MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "MiniMaxAI/MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "MiniMaxAI/MiniMax-M2.7"
                }
            }),
            description: "SiliconFlow 国际版".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "dmxapi".into(),
            name: "DMXAPI".into(),
            category: "aggregator".into(),
            icon: "dmxapi".into(),
            icon_color: "#6366F1".into(),
            website_url: "https://www.dmxapi.cn".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://www.dmxapi.cn",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "DMXAPI 聚合".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            category: "aggregator".into(),
            icon: "openrouter".into(),
            icon_color: "#6566F1".into(),
            website_url: "https://openrouter.ai".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "anthropic/claude-sonnet-4.6",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "anthropic/claude-haiku-4.5",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "anthropic/claude-sonnet-4.6",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "anthropic/claude-opus-4.7"
                }
            }),
            description: "OpenRouter 多模型路由".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "therouter".into(),
            name: "TheRouter".into(),
            category: "aggregator".into(),
            icon: "therouter".into(),
            icon_color: "#6366F1".into(),
            website_url: "https://therouter.ai".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.therouter.ai",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "anthropic/claude-sonnet-4.6",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "anthropic/claude-haiku-4.5",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "anthropic/claude-sonnet-4.6",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "anthropic/claude-opus-4.7"
                }
            }),
            description: "TheRouter AI 路由".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "novita".into(),
            name: "Novita AI".into(),
            category: "aggregator".into(),
            icon: "novita".into(),
            icon_color: "#000000".into(),
            website_url: "https://novita.ai".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.novita.ai/anthropic",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "zai-org/glm-5",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "zai-org/glm-5",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "zai-org/glm-5",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "zai-org/glm-5"
                }
            }),
            description: "Novita AI 聚合平台".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "nvidia".into(),
            name: "Nvidia".into(),
            category: "aggregator".into(),
            icon: "nvidia".into(),
            icon_color: "#76B900".into(),
            website_url: "https://build.nvidia.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://integrate.api.nvidia.com",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "moonshotai/kimi-k2.5",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "moonshotai/kimi-k2.5",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "moonshotai/kimi-k2.5",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "moonshotai/kimi-k2.5"
                }
            }),
            description: "Nvidia NIM 推理平台".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "pipellm".into(),
            name: "PIPELLM".into(),
            category: "aggregator".into(),
            icon: "pipellm".into(),
            icon_color: "#6366F1".into(),
            website_url: "https://code.pipellm.ai".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://cc-api.pipellm.ai",
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_MODEL": "claude-opus-4-7",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "claude-haiku-4-5-20251001",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-4-6",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-opus-4-7"
                }
            }),
            description: "PIPELLM 编程聚合".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "compshare".into(),
            name: "优云智算".into(),
            category: "aggregator".into(),
            icon: "ucloud".into(),
            icon_color: "#000000".into(),
            website_url: "https://www.compshare.cn".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.modelverse.cn",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "优云智算 (UCloud)".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "compshare-coding".into(),
            name: "优云智算 Coding".into(),
            category: "aggregator".into(),
            icon: "ucloud".into(),
            icon_color: "#000000".into(),
            website_url: "https://www.compshare.cn".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://cp.compshare.cn",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "优云智算 Coding Plan".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "runapi".into(),
            name: "RunAPI".into(),
            category: "aggregator".into(),
            icon: "runapi".into(),
            icon_color: "#6366F1".into(),
            website_url: "https://runapi.co".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://runapi.co",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "RunAPI 聚合".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        // ========================
        // Third Party
        // ========================
        ProviderPreset {
            id: "pateway".into(),
            name: "PatewayAI".into(),
            category: "third_party".into(),
            icon: "pateway".into(),
            icon_color: "#6366F1".into(),
            website_url: "https://pateway.ai".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.pateway.ai",
                    "ANTHROPIC_API_KEY": ""
                }
            }),
            description: "PatewayAI 代理".into(),
            supported_apps: apps_claude(),
        },
        ProviderPreset {
            id: "packycode".into(),
            name: "PackyCode".into(),
            category: "third_party".into(),
            icon: "packycode".into(),
            icon_color: "#6366F1".into(),
            website_url: "https://www.packyapi.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://www.packyapi.com",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "PackyCode 代理".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "claudeapi".into(),
            name: "ClaudeAPI".into(),
            category: "third_party".into(),
            icon: "claudeapi".into(),
            icon_color: "#D4915D".into(),
            website_url: "https://claudeapi.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://gw.claudeapi.com",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "ClaudeAPI 代理".into(),
            supported_apps: apps_claude(),
        },
        ProviderPreset {
            id: "claudecn".into(),
            name: "ClaudeCN".into(),
            category: "third_party".into(),
            icon: "claudecn".into(),
            icon_color: "#D4915D".into(),
            website_url: "https://claudecn.top".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://claudecn.top",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "ClaudeCN 国内加速".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "relaxycode".into(),
            name: "RelaxyCode".into(),
            category: "third_party".into(),
            icon: "relaxcode".into(),
            icon_color: "#6366F1".into(),
            website_url: "https://www.relaxycode.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://www.relaxycode.com",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "RelaxyCode 代理".into(),
            supported_apps: apps_claude(),
        },
        ProviderPreset {
            id: "cubence".into(),
            name: "Cubence".into(),
            category: "third_party".into(),
            icon: "cubence".into(),
            icon_color: "#000000".into(),
            website_url: "https://cubence.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.cubence.com",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "Cubence 代理 (多节点)".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "aigocode".into(),
            name: "AIGoCode".into(),
            category: "third_party".into(),
            icon: "aigocode".into(),
            icon_color: "#5B7FFF".into(),
            website_url: "https://aigocode.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.aigocode.com",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "AIGoCode 代理".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "rightcode".into(),
            name: "RightCode".into(),
            category: "third_party".into(),
            icon: "rc".into(),
            icon_color: "#E96B2C".into(),
            website_url: "https://www.right.codes".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://www.right.codes/claude",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "RightCode 代理".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "aicodemirror".into(),
            name: "AICodeMirror".into(),
            category: "third_party".into(),
            icon: "aicodemirror".into(),
            icon_color: "#000000".into(),
            website_url: "https://www.aicodemirror.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.aicodemirror.com/api/claudecode",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "AICodeMirror 代理".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "aicoding".into(),
            name: "AICoding".into(),
            category: "third_party".into(),
            icon: "aicoding".into(),
            icon_color: "#000000".into(),
            website_url: "https://aicoding.sh".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.aicoding.sh",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "AICoding 代理".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "crazyrouter".into(),
            name: "CrazyRouter".into(),
            category: "third_party".into(),
            icon: "crazyrouter".into(),
            icon_color: "#000000".into(),
            website_url: "https://www.crazyrouter.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://cn.crazyrouter.com",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "CrazyRouter 代理".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "sssaicode".into(),
            name: "SSSAiCode".into(),
            category: "third_party".into(),
            icon: "sssaicode".into(),
            icon_color: "#000000".into(),
            website_url: "https://www.sssaicode.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://node-hk.sssaicode.com/api",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "SSSAiCode 代理 (多节点)".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "micu".into(),
            name: "Micu".into(),
            category: "third_party".into(),
            icon: "micu".into(),
            icon_color: "#000000".into(),
            website_url: "https://www.micuapi.ai".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://www.micuapi.ai",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "Micu API 代理".into(),
            supported_apps: apps_claude_opencode_hermes(),
        },
        ProviderPreset {
            id: "ctok".into(),
            name: "CTok.ai".into(),
            category: "third_party".into(),
            icon: "ctok".into(),
            icon_color: "#000000".into(),
            website_url: "https://ctok.ai".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.ctok.ai",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "CTok.ai 代理".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "eflowcode".into(),
            name: "E-FlowCode".into(),
            category: "third_party".into(),
            icon: "eflowcode".into(),
            icon_color: "#000000".into(),
            website_url: "https://e-flowcode.cc".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "",
                    "ANTHROPIC_BASE_URL": "https://e-flowcode.cc"
                }
            }),
            description: "E-FlowCode 代理".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "lemondata".into(),
            name: "LemonData".into(),
            category: "third_party".into(),
            icon: "lemondata".into(),
            icon_color: "#FFD700".into(),
            website_url: "https://lemondata.cc".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.lemondata.cc",
                    "ANTHROPIC_API_KEY": ""
                }
            }),
            description: "LemonData 代理".into(),
            supported_apps: apps_claude_opencode_gemini_hermes(),
        },
        ProviderPreset {
            id: "github-copilot".into(),
            name: "GitHub Copilot".into(),
            category: "third_party".into(),
            icon: "github".into(),
            icon_color: "#000000".into(),
            website_url: "https://github.com/features/copilot".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.githubcopilot.com",
                    "ANTHROPIC_MODEL": "claude-sonnet-4.6",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "claude-haiku-4.5",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-4.6",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-sonnet-4.6"
                }
            }),
            description: "GitHub Copilot (需 OAuth 认证)".into(),
            supported_apps: apps_claude(),
        },
        ProviderPreset {
            id: "codex-oauth".into(),
            name: "Codex (ChatGPT)".into(),
            category: "third_party".into(),
            icon: "openai".into(),
            icon_color: "#000000".into(),
            website_url: "https://openai.com/chatgpt/pricing".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://chatgpt.com/backend-api/codex",
                    "ANTHROPIC_MODEL": "gpt-5.4",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "gpt-5.4-mini",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "gpt-5.4",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "gpt-5.4"
                }
            }),
            description: "OpenAI Codex via ChatGPT Plus/Pro".into(),
            supported_apps: apps_claude(),
        },
        // ========================
        // Cloud Provider
        // ========================
        ProviderPreset {
            id: "aws-bedrock-aksk".into(),
            name: "AWS Bedrock (AKSK)".into(),
            category: "cloud_provider".into(),
            icon: "aws".into(),
            icon_color: "#FF9900".into(),
            website_url: "https://aws.amazon.com/bedrock/".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://bedrock-runtime.us-west-2.amazonaws.com",
                    "AWS_ACCESS_KEY_ID": "",
                    "AWS_SECRET_ACCESS_KEY": "",
                    "AWS_REGION": "us-west-2",
                    "ANTHROPIC_MODEL": "global.anthropic.claude-opus-4-7",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "global.anthropic.claude-haiku-4-5-20251001-v1:0",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "global.anthropic.claude-sonnet-4-6",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "global.anthropic.claude-opus-4-7",
                    "CLAUDE_CODE_USE_BEDROCK": "1"
                }
            }),
            description: "AWS Bedrock (Access Key 认证)".into(),
            supported_apps: apps_claude_opencode(),
        },
        ProviderPreset {
            id: "aws-bedrock-apikey".into(),
            name: "AWS Bedrock (API Key)".into(),
            category: "cloud_provider".into(),
            icon: "aws".into(),
            icon_color: "#FF9900".into(),
            website_url: "https://aws.amazon.com/bedrock/".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://bedrock-runtime.us-west-2.amazonaws.com",
                    "AWS_REGION": "us-west-2",
                    "ANTHROPIC_MODEL": "global.anthropic.claude-opus-4-7",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "global.anthropic.claude-haiku-4-5-20251001-v1:0",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "global.anthropic.claude-sonnet-4-6",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "global.anthropic.claude-opus-4-7",
                    "CLAUDE_CODE_USE_BEDROCK": "1"
                },
                "apiKey": ""
            }),
            description: "AWS Bedrock (API Key 认证)".into(),
            supported_apps: apps_claude_opencode(),
        },
        ProviderPreset {
            id: "azure-openai".into(),
            name: "Azure OpenAI".into(),
            category: "cloud_provider".into(),
            icon: "azure".into(),
            icon_color: "#0078D4".into(),
            website_url: "https://azure.microsoft.com/products/ai-services/openai-service".into(),
            settings_template: json!({
                "primaryApiKey": "",
                "baseUrl": "",
                "env": {
                    "AZURE_OPENAI_API_KEY": "",
                    "AZURE_OPENAI_ENDPOINT": ""
                }
            }),
            description: "Microsoft Azure OpenAI 服务".into(),
            supported_apps: vec!["codex".into()],
        },
        // ========================
        // Hermes-only providers
        // ========================
        ProviderPreset {
            id: "together-ai".into(),
            name: "Together AI".into(),
            category: "aggregator".into(),
            icon: "together".into(),
            icon_color: "#0F6FFF".into(),
            website_url: "https://together.ai".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.together.xyz/v1",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "Together AI 推理平台".into(),
            supported_apps: vec!["hermes".into()],
        },
        ProviderPreset {
            id: "nous-research".into(),
            name: "Nous Research".into(),
            category: "official".into(),
            icon: "hermes".into(),
            icon_color: "#7C3AED".into(),
            website_url: "https://nousresearch.com".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://inference-api.nousresearch.com/v1",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "Nous Research Hermes 官方".into(),
            supported_apps: vec!["hermes".into()],
        },
        // ========================
        // Universal (NewAPI)
        // ========================
        ProviderPreset {
            id: "newapi".into(),
            name: "NewAPI".into(),
            category: "aggregator".into(),
            icon: "newapi".into(),
            icon_color: "#00A67E".into(),
            website_url: "https://www.newapi.pro".into(),
            settings_template: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "",
                    "ANTHROPIC_AUTH_TOKEN": ""
                }
            }),
            description: "NewAPI 自部署 API 网关 (支持多协议)".into(),
            supported_apps: apps_all(),
        },
    ]
}
