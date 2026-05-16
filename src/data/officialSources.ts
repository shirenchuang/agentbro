// ─── Types ───────────────────────────────────────────────────────────────────

export interface OfficialPublisher {
  name: string;
  slug: string; // GitHub org name
  totalSkills: number;
  repos: OfficialRepo[];
}

export interface OfficialRepo {
  fullName: string; // "owner/repo"
  url: string;
  skillCount: number;
  description?: string;
}

export type SkillTag =
  | "frontend"
  | "backend"
  | "ecommerce"
  | "app"
  | "devops"
  | "ai-ml"
  | "database"
  | "security"
  | "testing"
  | "docs";

export interface RecommendedSkill {
  name: string;
  description: string;
  publisher: string;
  repoFullName: string;
  tags: SkillTag[];
  downloadUrl: string;
}

// ─── Tag Labels ──────────────────────────────────────────────────────────────

export const TAG_LABELS: Record<SkillTag, { zh: string; en: string }> = {
  frontend: { zh: "前端开发", en: "Frontend" },
  backend: { zh: "后端开发", en: "Backend" },
  ecommerce: { zh: "电商", en: "E-commerce" },
  app: { zh: "App 开发", en: "App Dev" },
  devops: { zh: "DevOps", en: "DevOps" },
  "ai-ml": { zh: "AI/ML", en: "AI/ML" },
  database: { zh: "数据库", en: "Database" },
  security: { zh: "安全", en: "Security" },
  testing: { zh: "测试/监控", en: "Testing" },
  docs: { zh: "文档/设计", en: "Docs/Design" },
};

// ─── Official Publishers (from skills.sh/official, sorted by totalSkills) ────

export const OFFICIAL_PUBLISHERS: OfficialPublisher[] = [
  {
    name: "Microsoft",
    slug: "microsoft",
    totalSkills: 404,
    repos: [
      { fullName: "microsoft/azure-skills", url: "https://github.com/microsoft/azure-skills", skillCount: 404 },
    ],
  },
  {
    name: "GitHub",
    slug: "github",
    totalSkills: 331,
    repos: [
      { fullName: "github/awesome-copilot", url: "https://github.com/github/awesome-copilot", skillCount: 331 },
    ],
  },
  {
    name: "Anthropic",
    slug: "anthropics",
    totalSkills: 289,
    repos: [
      { fullName: "anthropics/knowledge-work-plugins", url: "https://github.com/anthropics/knowledge-work-plugins", skillCount: 176 },
      { fullName: "anthropics/claude-plugins-official", url: "https://github.com/anthropics/claude-plugins-official", skillCount: 31 },
      { fullName: "anthropics/financial-services-plugins", url: "https://github.com/anthropics/financial-services-plugins", skillCount: 30 },
      { fullName: "anthropics/skills", url: "https://github.com/anthropics/skills", skillCount: 18 },
      { fullName: "anthropics/claude-code", url: "https://github.com/anthropics/claude-code", skillCount: 10 },
      { fullName: "anthropics/life-sciences", url: "https://github.com/anthropics/life-sciences", skillCount: 6 },
      { fullName: "anthropics/claude-agent-sdk-demos", url: "https://github.com/anthropics/claude-agent-sdk-demos", skillCount: 6 },
      { fullName: "anthropics/claude-cookbooks", url: "https://github.com/anthropics/claude-cookbooks", skillCount: 4 },
      { fullName: "anthropics/healthcare", url: "https://github.com/anthropics/healthcare", skillCount: 3 },
    ],
  },
  {
    name: "MiniMax",
    slug: "MiniMax-AI",
    totalSkills: 0,
    repos: [
      { fullName: "MiniMax-AI/skills", url: "https://github.com/MiniMax-AI/skills", skillCount: 0 },
    ],
  },
  {
    name: "Sentry",
    slug: "getsentry",
    totalSkills: 244,
    repos: [
      { fullName: "getsentry/skills", url: "https://github.com/getsentry/skills", skillCount: 244 },
    ],
  },
  {
    name: "Vercel Labs",
    slug: "vercel-labs",
    totalSkills: 214,
    repos: [
      { fullName: "vercel-labs/agent-skills", url: "https://github.com/vercel-labs/agent-skills", skillCount: 214 },
    ],
  },
  {
    name: "Firecrawl",
    slug: "firecrawl",
    totalSkills: 168,
    repos: [
      { fullName: "firecrawl/cli", url: "https://github.com/firecrawl/cli", skillCount: 168 },
    ],
  },
  {
    name: "PostHog",
    slug: "posthog",
    totalSkills: 137,
    repos: [
      { fullName: "posthog/posthog", url: "https://github.com/posthog/posthog", skillCount: 137 },
    ],
  },
  {
    name: "Vercel",
    slug: "vercel",
    totalSkills: 120,
    repos: [
      { fullName: "vercel/ai", url: "https://github.com/vercel/ai", skillCount: 120 },
    ],
  },
  {
    name: "OpenAI",
    slug: "openai",
    totalSkills: 118,
    repos: [
      { fullName: "openai/skills", url: "https://github.com/openai/skills", skillCount: 118 },
    ],
  },
  {
    name: "LangChain",
    slug: "langchain-ai",
    totalSkills: 81,
    repos: [
      { fullName: "langchain-ai/langchain-skills", url: "https://github.com/langchain-ai/langchain-skills", skillCount: 81 },
    ],
  },
  {
    name: "HashiCorp",
    slug: "hashicorp",
    totalSkills: 74,
    repos: [
      { fullName: "hashicorp/agent-skills", url: "https://github.com/hashicorp/agent-skills", skillCount: 74 },
    ],
  },
  {
    name: "Cloudflare",
    slug: "cloudflare",
    totalSkills: 59,
    repos: [
      { fullName: "cloudflare/skills", url: "https://github.com/cloudflare/skills", skillCount: 59 },
    ],
  },
  {
    name: "Flutter",
    slug: "flutter",
    totalSkills: 58,
    repos: [
      { fullName: "flutter/skills", url: "https://github.com/flutter/skills", skillCount: 58 },
    ],
  },
  {
    name: "Dagster",
    slug: "dagster-io",
    totalSkills: 53,
    repos: [
      { fullName: "dagster-io/skills", url: "https://github.com/dagster-io/skills", skillCount: 53 },
    ],
  },
  {
    name: "Prisma",
    slug: "prisma",
    totalSkills: 47,
    repos: [
      { fullName: "prisma/skills", url: "https://github.com/prisma/skills", skillCount: 47 },
    ],
  },
  {
    name: "Firebase",
    slug: "firebase",
    totalSkills: 40,
    repos: [
      { fullName: "firebase/agent-skills", url: "https://github.com/firebase/agent-skills", skillCount: 40 },
    ],
  },
  {
    name: "Bitwarden",
    slug: "bitwarden",
    totalSkills: 37,
    repos: [
      { fullName: "bitwarden/ai-plugins", url: "https://github.com/bitwarden/ai-plugins", skillCount: 37 },
    ],
  },
  {
    name: "Streamlit",
    slug: "streamlit",
    totalSkills: 36,
    repos: [
      { fullName: "streamlit/agent-skills", url: "https://github.com/streamlit/agent-skills", skillCount: 36 },
    ],
  },
  {
    name: "Pulumi",
    slug: "pulumi",
    totalSkills: 31,
    repos: [
      { fullName: "pulumi/agent-skills", url: "https://github.com/pulumi/agent-skills", skillCount: 31 },
    ],
  },
  {
    name: "Hugging Face",
    slug: "huggingface",
    totalSkills: 30,
    repos: [
      { fullName: "huggingface/skills", url: "https://github.com/huggingface/skills", skillCount: 30 },
    ],
  },
  {
    name: "Upstash",
    slug: "upstash",
    totalSkills: 30,
    repos: [
      { fullName: "upstash/context7", url: "https://github.com/upstash/context7", skillCount: 30 },
    ],
  },
  {
    name: "Sanity",
    slug: "sanity-io",
    totalSkills: 29,
    repos: [
      { fullName: "sanity-io/agent-toolkit", url: "https://github.com/sanity-io/agent-toolkit", skillCount: 29 },
    ],
  },
  {
    name: "Shopify",
    slug: "shopify",
    totalSkills: 28,
    repos: [
      { fullName: "shopify/shopify-ai-toolkit", url: "https://github.com/shopify/shopify-ai-toolkit", skillCount: 28 },
    ],
  },
  {
    name: "Runway ML",
    slug: "runwayml",
    totalSkills: 27,
    repos: [
      { fullName: "runwayml/skills", url: "https://github.com/runwayml/skills", skillCount: 27 },
    ],
  },
  {
    name: "Coinbase",
    slug: "coinbase",
    totalSkills: 27,
    repos: [
      { fullName: "coinbase/agentic-wallet-skills", url: "https://github.com/coinbase/agentic-wallet-skills", skillCount: 27 },
    ],
  },
  {
    name: "Automattic",
    slug: "automattic",
    totalSkills: 26,
    repos: [
      { fullName: "automattic/agent-skills", url: "https://github.com/automattic/agent-skills", skillCount: 26 },
    ],
  },
  {
    name: "Astronomer",
    slug: "astronomer",
    totalSkills: 26,
    repos: [
      { fullName: "astronomer/agents", url: "https://github.com/astronomer/agents", skillCount: 26 },
    ],
  },
  {
    name: "Clerk",
    slug: "clerk",
    totalSkills: 25,
    repos: [
      { fullName: "clerk/skills", url: "https://github.com/clerk/skills", skillCount: 25 },
    ],
  },
  {
    name: "Apify",
    slug: "apify",
    totalSkills: 23,
    repos: [
      { fullName: "apify/agent-skills", url: "https://github.com/apify/agent-skills", skillCount: 23 },
    ],
  },
  {
    name: "Notion",
    slug: "makenotion",
    totalSkills: 23,
    repos: [
      { fullName: "makenotion/claude-code-notion-plugin", url: "https://github.com/makenotion/claude-code-notion-plugin", skillCount: 23 },
    ],
  },
  {
    name: "Webflow",
    slug: "webflow",
    totalSkills: 23,
    repos: [
      { fullName: "webflow/webflow-skills", url: "https://github.com/webflow/webflow-skills", skillCount: 23 },
    ],
  },
  {
    name: "Mapbox",
    slug: "mapbox",
    totalSkills: 22,
    repos: [
      { fullName: "mapbox/mapbox-agent-skills", url: "https://github.com/mapbox/mapbox-agent-skills", skillCount: 22 },
    ],
  },
  {
    name: "Google Gemini",
    slug: "google-gemini",
    totalSkills: 21,
    repos: [
      { fullName: "google-gemini/gemini-skills", url: "https://github.com/google-gemini/gemini-skills", skillCount: 21 },
    ],
  },
  {
    name: "Base",
    slug: "base",
    totalSkills: 21,
    repos: [
      { fullName: "base/skills", url: "https://github.com/base/skills", skillCount: 21 },
    ],
  },
  {
    name: "Neon Database",
    slug: "neondatabase",
    totalSkills: 20,
    repos: [
      { fullName: "neondatabase/agent-skills", url: "https://github.com/neondatabase/agent-skills", skillCount: 20 },
    ],
  },
  {
    name: "Wix",
    slug: "wix",
    totalSkills: 19,
    repos: [
      { fullName: "wix/skills", url: "https://github.com/wix/skills", skillCount: 19 },
    ],
  },
  {
    name: "Tavily AI",
    slug: "tavily-ai",
    totalSkills: 19,
    repos: [
      { fullName: "tavily-ai/skills", url: "https://github.com/tavily-ai/skills", skillCount: 19 },
    ],
  },
  {
    name: "Encore",
    slug: "encoredev",
    totalSkills: 18,
    repos: [
      { fullName: "encoredev/skills", url: "https://github.com/encoredev/skills", skillCount: 18 },
    ],
  },
  {
    name: "Medusa",
    slug: "medusajs",
    totalSkills: 16,
    repos: [
      { fullName: "medusajs/medusa-agent-skills", url: "https://github.com/medusajs/medusa-agent-skills", skillCount: 16 },
    ],
  },
  {
    name: "Datadog",
    slug: "datadog-labs",
    totalSkills: 16,
    repos: [
      { fullName: "datadog-labs/agent-skills", url: "https://github.com/datadog-labs/agent-skills", skillCount: 16 },
    ],
  },
  {
    name: "LaunchDarkly",
    slug: "launchdarkly",
    totalSkills: 16,
    repos: [
      { fullName: "launchdarkly/agent-skills", url: "https://github.com/launchdarkly/agent-skills", skillCount: 16 },
    ],
  },
  {
    name: "Callstack",
    slug: "callstackincubator",
    totalSkills: 15,
    repos: [
      { fullName: "callstackincubator/agent-skills", url: "https://github.com/callstackincubator/agent-skills", skillCount: 15 },
    ],
  },
  {
    name: "Langfuse",
    slug: "langfuse",
    totalSkills: 15,
    repos: [
      { fullName: "langfuse/skills", url: "https://github.com/langfuse/skills", skillCount: 15 },
    ],
  },
  {
    name: "Auth0",
    slug: "auth0",
    totalSkills: 14,
    repos: [
      { fullName: "auth0/agent-skills", url: "https://github.com/auth0/agent-skills", skillCount: 14 },
    ],
  },
  {
    name: "Mastra AI",
    slug: "mastra-ai",
    totalSkills: 14,
    repos: [
      { fullName: "mastra-ai/skills", url: "https://github.com/mastra-ai/skills", skillCount: 14 },
    ],
  },
  {
    name: "Supabase",
    slug: "supabase",
    totalSkills: 14,
    repos: [
      { fullName: "supabase/agent-skills", url: "https://github.com/supabase/agent-skills", skillCount: 14 },
    ],
  },
  {
    name: "WordPress",
    slug: "wordpress",
    totalSkills: 14,
    repos: [
      { fullName: "wordpress/agent-skills", url: "https://github.com/wordpress/agent-skills", skillCount: 14 },
    ],
  },
  {
    name: "Apollo GraphQL",
    slug: "apollographql",
    totalSkills: 13,
    repos: [
      { fullName: "apollographql/skills", url: "https://github.com/apollographql/skills", skillCount: 13 },
    ],
  },
  {
    name: "ClickHouse",
    slug: "clickhouse",
    totalSkills: 13,
    repos: [
      { fullName: "clickhouse/agent-skills", url: "https://github.com/clickhouse/agent-skills", skillCount: 13 },
    ],
  },
  {
    name: "dbt Labs",
    slug: "dbt-labs",
    totalSkills: 13,
    repos: [
      { fullName: "dbt-labs/dbt-agent-skills", url: "https://github.com/dbt-labs/dbt-agent-skills", skillCount: 13 },
    ],
  },
  {
    name: "Expo",
    slug: "expo",
    totalSkills: 13,
    repos: [
      { fullName: "expo/skills", url: "https://github.com/expo/skills", skillCount: 13 },
    ],
  },
  {
    name: "Resend",
    slug: "resend",
    totalSkills: 13,
    repos: [
      { fullName: "resend/resend-skills", url: "https://github.com/resend/resend-skills", skillCount: 13 },
    ],
  },
  {
    name: "Trigger.dev",
    slug: "triggerdotdev",
    totalSkills: 13,
    repos: [
      { fullName: "triggerdotdev/skills", url: "https://github.com/triggerdotdev/skills", skillCount: 13 },
    ],
  },
  {
    name: "tldraw",
    slug: "tldraw",
    totalSkills: 13,
    repos: [
      { fullName: "tldraw/tldraw", url: "https://github.com/tldraw/tldraw", skillCount: 13 },
    ],
  },
  {
    name: "Brave",
    slug: "brave",
    totalSkills: 11,
    repos: [
      { fullName: "brave/brave-search-skills", url: "https://github.com/brave/brave-search-skills", skillCount: 11 },
    ],
  },
  {
    name: "Better Auth",
    slug: "better-auth",
    totalSkills: 11,
    repos: [
      { fullName: "better-auth/skills", url: "https://github.com/better-auth/skills", skillCount: 11 },
    ],
  },
  {
    name: "React (Facebook)",
    slug: "facebook",
    totalSkills: 11,
    repos: [
      { fullName: "facebook/react", url: "https://github.com/facebook/react", skillCount: 11 },
    ],
  },
  {
    name: "Figma",
    slug: "figma",
    totalSkills: 11,
    repos: [
      { fullName: "figma/mcp-server-guide", url: "https://github.com/figma/mcp-server-guide", skillCount: 11 },
    ],
  },
  {
    name: "Rivet",
    slug: "rivet-dev",
    totalSkills: 11,
    repos: [
      { fullName: "rivet-dev/skills", url: "https://github.com/rivet-dev/skills", skillCount: 11 },
    ],
  },
  {
    name: "n8n",
    slug: "n8n-io",
    totalSkills: 11,
    repos: [
      { fullName: "n8n-io/n8n", url: "https://github.com/n8n-io/n8n", skillCount: 11 },
    ],
  },
  {
    name: "Stripe",
    slug: "stripe",
    totalSkills: 10,
    repos: [
      { fullName: "stripe/ai", url: "https://github.com/stripe/ai", skillCount: 10 },
    ],
  },
  {
    name: "ElevenLabs",
    slug: "elevenlabs",
    totalSkills: 9,
    repos: [
      { fullName: "elevenlabs/skills", url: "https://github.com/elevenlabs/skills", skillCount: 9 },
    ],
  },
  {
    name: "Remotion",
    slug: "remotion-dev",
    totalSkills: 9,
    repos: [
      { fullName: "remotion-dev/skills", url: "https://github.com/remotion-dev/skills", skillCount: 9 },
    ],
  },
  {
    name: "PlanetScale",
    slug: "planetscale",
    totalSkills: 8,
    repos: [
      { fullName: "planetscale/database-skills", url: "https://github.com/planetscale/database-skills", skillCount: 8 },
    ],
  },
  {
    name: "Axiom",
    slug: "axiomhq",
    totalSkills: 7,
    repos: [
      { fullName: "axiomhq/skills", url: "https://github.com/axiomhq/skills", skillCount: 7 },
    ],
  },
  {
    name: "Browserbase",
    slug: "browserbase",
    totalSkills: 7,
    repos: [
      { fullName: "browserbase/skills", url: "https://github.com/browserbase/skills", skillCount: 7 },
    ],
  },
  {
    name: "Deno",
    slug: "denoland",
    totalSkills: 6,
    repos: [
      { fullName: "denoland/skills", url: "https://github.com/denoland/skills", skillCount: 6 },
    ],
  },
  {
    name: "Redis",
    slug: "redis",
    totalSkills: 6,
    repos: [
      { fullName: "redis/agent-skills", url: "https://github.com/redis/agent-skills", skillCount: 6 },
    ],
  },
  {
    name: "Semgrep",
    slug: "semgrep",
    totalSkills: 6,
    repos: [
      { fullName: "semgrep/skills", url: "https://github.com/semgrep/skills", skillCount: 6 },
    ],
  },
];

// ─── Recommended Skills (curated, with tags) ─────────────────────────────────

export const RECOMMENDED_SKILLS: RecommendedSkill[] = [
  // Frontend
  {
    name: "web-artifacts-builder",
    description: "Suite of tools for creating elaborate, multi-component HTML artifacts using React, Tailwind CSS, shadcn/ui",
    publisher: "Anthropic",
    repoFullName: "anthropics/skills",
    tags: ["frontend"],
    downloadUrl: "https://raw.githubusercontent.com/anthropics/skills/main/web-artifacts-builder/SKILL.md",
  },
  {
    name: "frontend-design",
    description: "Create distinctive, production-grade frontend interfaces with high design quality",
    publisher: "Anthropic",
    repoFullName: "anthropics/skills",
    tags: ["frontend"],
    downloadUrl: "https://raw.githubusercontent.com/anthropics/skills/main/frontend-design/SKILL.md",
  },
  {
    name: "flutter-dev",
    description: "Build cross-platform Flutter apps with Dart, state management, and performance optimization",
    publisher: "Flutter",
    repoFullName: "flutter/skills",
    tags: ["frontend", "app"],
    downloadUrl: "https://raw.githubusercontent.com/flutter/skills/main/flutter-dev/SKILL.md",
  },
  {
    name: "expo-development",
    description: "Build React Native apps with Expo, including routing, native modules, and deployment",
    publisher: "Expo",
    repoFullName: "expo/skills",
    tags: ["frontend", "app"],
    downloadUrl: "https://raw.githubusercontent.com/expo/skills/main/expo-development/SKILL.md",
  },
  // Backend
  {
    name: "cloudflare-workers",
    description: "Build and deploy Cloudflare Workers, Pages, D1, R2, and KV with best practices",
    publisher: "Cloudflare",
    repoFullName: "cloudflare/skills",
    tags: ["backend"],
    downloadUrl: "https://raw.githubusercontent.com/cloudflare/skills/main/cloudflare-workers/SKILL.md",
  },
  {
    name: "firebase-development",
    description: "Firebase Authentication, Firestore, Cloud Functions setup and integration",
    publisher: "Firebase",
    repoFullName: "firebase/agent-skills",
    tags: ["backend"],
    downloadUrl: "https://raw.githubusercontent.com/firebase/agent-skills/main/firebase-development/SKILL.md",
  },
  {
    name: "supabase-development",
    description: "Build with Supabase — Postgres, Auth, Edge Functions, Realtime, and Storage",
    publisher: "Supabase",
    repoFullName: "supabase/agent-skills",
    tags: ["backend", "database"],
    downloadUrl: "https://raw.githubusercontent.com/supabase/agent-skills/main/supabase-development/SKILL.md",
  },
  {
    name: "deno-development",
    description: "Build modern server-side applications with Deno runtime",
    publisher: "Deno",
    repoFullName: "denoland/skills",
    tags: ["backend"],
    downloadUrl: "https://raw.githubusercontent.com/denoland/skills/main/deno-development/SKILL.md",
  },
  // E-commerce
  {
    name: "stripe-integration",
    description: "Stripe payment integration — checkout, subscriptions, webhooks, and PCI compliance",
    publisher: "Stripe",
    repoFullName: "stripe/ai",
    tags: ["ecommerce"],
    downloadUrl: "https://raw.githubusercontent.com/stripe/ai/main/stripe-integration/SKILL.md",
  },
  {
    name: "shopify-development",
    description: "Build Shopify apps, themes, and storefronts with Shopify AI toolkit",
    publisher: "Shopify",
    repoFullName: "shopify/shopify-ai-toolkit",
    tags: ["ecommerce"],
    downloadUrl: "https://raw.githubusercontent.com/shopify/shopify-ai-toolkit/main/shopify-development/SKILL.md",
  },
  // AI/ML
  {
    name: "langchain-development",
    description: "Build LLM applications with LangChain — chains, agents, retrievers, and tools",
    publisher: "LangChain",
    repoFullName: "langchain-ai/langchain-skills",
    tags: ["ai-ml"],
    downloadUrl: "https://raw.githubusercontent.com/langchain-ai/langchain-skills/main/langchain-development/SKILL.md",
  },
  {
    name: "huggingface-models",
    description: "Work with Hugging Face models, datasets, and transformers pipeline",
    publisher: "Hugging Face",
    repoFullName: "huggingface/skills",
    tags: ["ai-ml"],
    downloadUrl: "https://raw.githubusercontent.com/huggingface/skills/main/huggingface-models/SKILL.md",
  },
  // DevOps
  {
    name: "terraform-skills",
    description: "Infrastructure as Code with HashiCorp Terraform — providers, modules, state management",
    publisher: "HashiCorp",
    repoFullName: "hashicorp/agent-skills",
    tags: ["devops"],
    downloadUrl: "https://raw.githubusercontent.com/hashicorp/agent-skills/main/terraform-skills/SKILL.md",
  },
  {
    name: "pulumi-development",
    description: "Cloud infrastructure with Pulumi — TypeScript/Python/Go, state management, CI/CD",
    publisher: "Pulumi",
    repoFullName: "pulumi/agent-skills",
    tags: ["devops"],
    downloadUrl: "https://raw.githubusercontent.com/pulumi/agent-skills/main/pulumi-development/SKILL.md",
  },
  // Database
  {
    name: "prisma-orm",
    description: "Database development with Prisma ORM — schema, migrations, queries, and relations",
    publisher: "Prisma",
    repoFullName: "prisma/skills",
    tags: ["database"],
    downloadUrl: "https://raw.githubusercontent.com/prisma/skills/main/prisma-orm/SKILL.md",
  },
  {
    name: "redis-development",
    description: "Redis data structures, caching strategies, Pub/Sub, and Streams",
    publisher: "Redis",
    repoFullName: "redis/agent-skills",
    tags: ["database"],
    downloadUrl: "https://raw.githubusercontent.com/redis/agent-skills/main/redis-development/SKILL.md",
  },
  // Security
  {
    name: "auth0-integration",
    description: "Auth0 authentication and authorization — SSO, MFA, user management",
    publisher: "Auth0",
    repoFullName: "auth0/agent-skills",
    tags: ["security"],
    downloadUrl: "https://raw.githubusercontent.com/auth0/agent-skills/main/auth0-integration/SKILL.md",
  },
  {
    name: "clerk-auth",
    description: "Clerk authentication — components, middleware, user management, organizations",
    publisher: "Clerk",
    repoFullName: "clerk/skills",
    tags: ["security"],
    downloadUrl: "https://raw.githubusercontent.com/clerk/skills/main/clerk-auth/SKILL.md",
  },
  // Testing / Monitoring
  {
    name: "sentry-monitoring",
    description: "Error tracking and performance monitoring with Sentry",
    publisher: "Sentry",
    repoFullName: "getsentry/skills",
    tags: ["testing"],
    downloadUrl: "https://raw.githubusercontent.com/getsentry/skills/main/sentry-monitoring/SKILL.md",
  },
  {
    name: "datadog-monitoring",
    description: "Application monitoring, logging, and tracing with Datadog",
    publisher: "Datadog",
    repoFullName: "datadog-labs/agent-skills",
    tags: ["testing"],
    downloadUrl: "https://raw.githubusercontent.com/datadog-labs/agent-skills/main/datadog-monitoring/SKILL.md",
  },
  // Docs / Design
  {
    name: "notion-integration",
    description: "Notion API integration — databases, pages, blocks, and search",
    publisher: "Notion",
    repoFullName: "makenotion/claude-code-notion-plugin",
    tags: ["docs"],
    downloadUrl: "https://raw.githubusercontent.com/makenotion/claude-code-notion-plugin/main/notion-integration/SKILL.md",
  },
  {
    name: "figma-development",
    description: "Figma plugin development and design-to-code workflows",
    publisher: "Figma",
    repoFullName: "figma/mcp-server-guide",
    tags: ["docs"],
    downloadUrl: "https://raw.githubusercontent.com/figma/mcp-server-guide/main/figma-development/SKILL.md",
  },
];

// ─── All Tags ────────────────────────────────────────────────────────────────

export const ALL_TAGS: SkillTag[] = [
  "frontend",
  "backend",
  "ecommerce",
  "app",
  "devops",
  "ai-ml",
  "database",
  "security",
  "testing",
  "docs",
];
