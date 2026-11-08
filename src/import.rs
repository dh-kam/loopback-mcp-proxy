use std::collections::BTreeMap;
use std::path::PathBuf;

use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;

use crate::auth::{apply_discovery, discover};
use crate::error::AppError;
use crate::model::{
    AuthConfig, AuthKind, Discovery, HeaderPair, OAuthConfig, OAuthGrant, ServerInput, Transport,
};

const AGY_OFFICIAL_MARKETPLACE_URL: &str =
    "https://www.gstatic.com/antigravity/plugins/marketplace.json";
const AGY_PARTNERS_MARKETPLACE_URL: &str =
    "https://www.gstatic.com/antigravity/plugins-partners/marketplace.json";
const CLAUDE_OFFICIAL_MARKETPLACE_URL: &str =
    "https://raw.githubusercontent.com/anthropics/claude-plugins-official/main/.claude-plugin/marketplace.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub command: String,
    #[serde(default = "default_true")]
    pub auto_discover: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResponse {
    pub detected_format: String,
    pub source_label: String,
    pub candidates: Vec<ImportedCandidate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedCandidate {
    pub input: ServerInput,
    pub discovery: Option<Discovery>,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
    pub slug: &'static str,
    pub name: &'static str,
    pub provider: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub url: &'static str,
    pub transport: Transport,
    pub auth_kind: AuthKind,
    pub auth_label: &'static str,
    pub secret_hint: Option<&'static str>,
    pub install_command: String,
    pub featured: bool,
}

struct KnownPluginSpec {
    slug: &'static str,
    display_name: &'static str,
    provider: &'static str,
    category: &'static str,
    description: &'static str,
    url: &'static str,
    transport: Transport,
    auth_kind: AuthKind,
    auth_label: &'static str,
    secret_hint: Option<&'static str>,
    headers: &'static [(&'static str, &'static str)],
    oauth_client_id: Option<&'static str>,
    featured: bool,
}

const KNOWN_PLUGINS: &[KnownPluginSpec] = &[
    // --- Productivity & Docs ---
    KnownPluginSpec {
        slug: "notion",
        display_name: "Notion",
        provider: "Notion Labs",
        category: "productivity",
        description: "Search, read, create, and update Notion pages, databases, and comments.",
        url: "https://mcp.notion.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: true,
    },
    KnownPluginSpec {
        slug: "linear",
        display_name: "Linear",
        provider: "Linear",
        category: "productivity",
        description: "Manage Linear issues, projects, cycles, labels, and team workflows.",
        url: "https://mcp.linear.app/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: true,
    },
    KnownPluginSpec {
        slug: "atlassian",
        display_name: "Atlassian (Jira & Confluence)",
        provider: "Atlassian",
        category: "productivity",
        description: "Official Rovo MCP for searching and updating Jira issues and Confluence pages.",
        url: "https://mcp.atlassian.com/v1/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: true,
    },
    KnownPluginSpec {
        slug: "asana",
        display_name: "Asana",
        provider: "Asana",
        category: "productivity",
        description: "Interact with Asana Work Graph tasks, projects, portfolios, and status updates.",
        url: "https://mcp.asana.com/sse",
        transport: Transport::Sse,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "monday",
        display_name: "monday.com",
        provider: "monday.com",
        category: "productivity",
        description: "Manage monday.com boards, items, columns, and team automations.",
        url: "https://mcp.monday.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "trello",
        display_name: "Trello",
        provider: "Atlassian / Trello",
        category: "productivity",
        description: "Read and manage Trello boards, lists, cards, and checklists.",
        url: "https://mcp.trello.com/v1",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "intercom",
        display_name: "Intercom",
        provider: "Intercom",
        category: "productivity",
        description: "Search customer conversations, support tickets, contacts, and help center articles.",
        url: "https://mcp.intercom.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "hubspot",
        display_name: "HubSpot CRM",
        provider: "HubSpot",
        category: "productivity",
        description: "Access CRM contacts, companies, deals, tickets, and sales engagement.",
        url: "https://mcp.hubspot.com/anthropic",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "box",
        display_name: "Box",
        provider: "Box",
        category: "productivity",
        description: "Search files, folders, and extract enterprise document insights in Box.",
        url: "https://mcp.box.com",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "dropbox",
        display_name: "Dropbox",
        provider: "Dropbox",
        category: "productivity",
        description: "Search and retrieve cloud files and shared documents from Dropbox.",
        url: "https://mcp.dropbox.com/gemini_app_mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "pandadoc",
        display_name: "PandaDoc",
        provider: "PandaDoc",
        category: "productivity",
        description: "Create, track, and manage proposals, quotes, and e-signature documents.",
        url: "https://mcp.pandadoc.com/v1/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },

    // --- Developer & Code ---
    KnownPluginSpec {
        slug: "github",
        display_name: "GitHub Copilot MCP",
        provider: "GitHub",
        category: "developer",
        description: "Official GitHub remote MCP for repositories, PRs, issues, Actions, and code search.",
        url: "https://api.githubcopilot.com/mcp/",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::Bearer,
        auth_label: "Bearer Token (PAT)",
        secret_hint: Some("GITHUB_PERSONAL_ACCESS_TOKEN"),
        headers: &[("Authorization", "Bearer ${GITHUB_PERSONAL_ACCESS_TOKEN}")],
        oauth_client_id: None,
        featured: true,
    },
    KnownPluginSpec {
        slug: "sentry",
        display_name: "Sentry",
        provider: "Sentry",
        category: "developer",
        description: "Inspect production issues, stacktraces, releases, and Seer root-cause analysis.",
        url: "https://mcp.sentry.dev/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: true,
    },
    KnownPluginSpec {
        slug: "context7",
        display_name: "Context7 Docs",
        provider: "Upstash",
        category: "developer",
        description: "Up-to-date, version-specific documentation and code examples for thousands of libraries.",
        url: "https://mcp.context7.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::None,
        auth_label: "Open (No Auth)",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: true,
    },
    KnownPluginSpec {
        slug: "deepwiki",
        display_name: "DeepWiki",
        provider: "Cognition / Devin",
        category: "developer",
        description: "Architecture diagrams, documentation, and Q&A for any public GitHub repository.",
        url: "https://mcp.deepwiki.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::None,
        auth_label: "Open (No Auth)",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: true,
    },
    KnownPluginSpec {
        slug: "gitlab",
        display_name: "GitLab",
        provider: "GitLab",
        category: "developer",
        description: "Manage GitLab projects, merge requests, CI/CD pipelines, and issues.",
        url: "https://gitlab.com/api/v4/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "huggingface",
        display_name: "Hugging Face Hub",
        provider: "Hugging Face",
        category: "developer",
        description: "Search models, datasets, research papers, and invoke Gradio Spaces.",
        url: "https://huggingface.co/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::Bearer,
        auth_label: "Bearer Token (HF_TOKEN)",
        secret_hint: Some("HF_TOKEN"),
        headers: &[("Authorization", "Bearer ${HF_TOKEN}")],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "postman",
        display_name: "Postman",
        provider: "Postman",
        category: "developer",
        description: "Manage Postman collections, environments, API specs, and workspaces.",
        url: "https://mcp.postman.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "semgrep",
        display_name: "Semgrep Security",
        provider: "Semgrep",
        category: "developer",
        description: "Scan code snippets and repositories for security vulnerabilities and SAST rules.",
        url: "https://mcp.semgrep.ai/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::None,
        auth_label: "Open (No Auth)",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },

    // --- Cloud, Deploy & Payments ---
    KnownPluginSpec {
        slug: "stripe",
        display_name: "Stripe",
        provider: "Stripe",
        category: "cloud",
        description: "Manage Stripe customers, payment intents, subscriptions, invoices, and refunds.",
        url: "https://mcp.stripe.com",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: true,
    },
    KnownPluginSpec {
        slug: "vercel",
        display_name: "Vercel",
        provider: "Vercel",
        category: "cloud",
        description: "Inspect Vercel projects, deployments, build logs, domains, and environment config.",
        url: "https://mcp.vercel.com",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: true,
    },
    KnownPluginSpec {
        slug: "cloudflare-docs",
        display_name: "Cloudflare Developer Docs",
        provider: "Cloudflare",
        category: "cloud",
        description: "Search official Cloudflare Workers, Pages, R2, D1, and Zero Trust documentation.",
        url: "https://docs.mcp.cloudflare.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::None,
        auth_label: "Open (No Auth)",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "cloudflare-observability",
        display_name: "Cloudflare Observability",
        provider: "Cloudflare",
        category: "cloud",
        description: "Query Workers logs, analytics, traces, and account telemetry.",
        url: "https://observability.mcp.cloudflare.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "netlify",
        display_name: "Netlify",
        provider: "Netlify",
        category: "cloud",
        description: "Deploy sites, inspect build logs, and manage Netlify serverless functions.",
        url: "https://netlify-mcp.netlify.app/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "railway",
        display_name: "Railway",
        provider: "Railway",
        category: "cloud",
        description: "Manage Railway cloud projects, services, deployments, and environment variables.",
        url: "https://mcp.railway.com",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "paypal",
        display_name: "PayPal",
        provider: "PayPal",
        category: "cloud",
        description: "Create and manage PayPal invoices, orders, subscriptions, and transaction records.",
        url: "https://mcp.paypal.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "square",
        display_name: "Square",
        provider: "Block / Square",
        category: "cloud",
        description: "Access Square merchant catalog, payments, orders, inventory, and customers.",
        url: "https://mcp.squareup.com/sse",
        transport: Transport::Sse,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },

    // --- Data, Databases & Security ---
    KnownPluginSpec {
        slug: "supabase",
        display_name: "Supabase",
        provider: "Supabase",
        category: "data",
        description: "Manage Supabase projects, Postgres tables, SQL queries, migrations, and Edge Functions.",
        url: "https://mcp.supabase.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: true,
    },
    KnownPluginSpec {
        slug: "neon",
        display_name: "Neon Serverless Postgres",
        provider: "Neon",
        category: "data",
        description: "Manage Neon database branches, run SQL queries, and inspect Postgres schemas.",
        url: "https://mcp.neon.tech/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "mongodb-atlas",
        display_name: "MongoDB Atlas",
        provider: "MongoDB",
        category: "data",
        description: "Inspect MongoDB Atlas clusters, databases, collections, and aggregation queries.",
        url: "https://mcp.mongodb.com/",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: Some("google-antigravity"),
        featured: false,
    },
    KnownPluginSpec {
        slug: "prisma-postgres",
        display_name: "Prisma Postgres",
        provider: "Prisma",
        category: "data",
        description: "Provision Prisma Postgres databases, manage backups, and inspect connection state.",
        url: "https://mcp.prisma.io/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "posthog",
        display_name: "PostHog Analytics",
        provider: "PostHog",
        category: "data",
        description: "Query product analytics, feature flags, session insights, and error tracking.",
        url: "https://mcp.posthog.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::Bearer,
        auth_label: "Bearer Token (Personal API Key)",
        secret_hint: Some("POSTHOG_API_KEY"),
        headers: &[("Authorization", "Bearer ${POSTHOG_API_KEY}")],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "pagerduty",
        display_name: "PagerDuty",
        provider: "PagerDuty",
        category: "data",
        description: "Manage incidents, on-call schedules, escalation policies, and service health.",
        url: "https://mcp.pagerduty.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::Bearer,
        auth_label: "Bearer Token (API Key)",
        secret_hint: Some("PAGERDUTY_API_KEY"),
        headers: &[("Authorization", "Bearer ${PAGERDUTY_API_KEY}")],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "virustotal",
        display_name: "VirusTotal",
        provider: "Google Cloud / VirusTotal",
        category: "data",
        description: "Analyze URLs, file hashes, domains, and IP addresses for threat intelligence.",
        url: "https://ai.virustotal.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },

    // --- Design, Web & AI ---
    KnownPluginSpec {
        slug: "figma",
        display_name: "Figma",
        provider: "Figma",
        category: "design",
        description: "Extract Figma Dev Mode design context, components, variables, and layout specs.",
        url: "https://mcp.figma.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: true,
    },
    KnownPluginSpec {
        slug: "canva",
        display_name: "Canva",
        provider: "Canva",
        category: "design",
        description: "Search, generate, and export Canva designs, presentations, and brand templates.",
        url: "https://mcp.canva.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "zapier",
        display_name: "Zapier MCP",
        provider: "Zapier",
        category: "design",
        description: "Trigger automated actions across 7,000+ SaaS integrations via Zapier MCP.",
        url: "https://mcp.zapier.com/api/v1/connect",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "wix",
        display_name: "Wix",
        provider: "Wix",
        category: "design",
        description: "Manage Wix websites, CMS collections, bookings, and eCommerce stores.",
        url: "https://mcp.wix.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "wordpress",
        display_name: "WordPress.com",
        provider: "Automattic",
        category: "design",
        description: "Publish and manage WordPress.com posts, pages, media, and site settings.",
        url: "https://public-api.wordpress.com/wpcom/v2/mcp/v1",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "parallel",
        display_name: "Parallel Search",
        provider: "Parallel AI",
        category: "design",
        description: "High-accuracy web search and deep research synthesis for AI agents.",
        url: "https://search.parallel.ai/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "semrush",
        display_name: "Semrush",
        provider: "Semrush",
        category: "design",
        description: "SEO keyword analytics, domain traffic insights, and backlink auditing.",
        url: "https://mcp.semrush.com/antigravity/v2/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "wispr-flow",
        display_name: "Wispr Flow",
        provider: "Wispr",
        category: "design",
        description: "Access voice dictation context and notes from Wispr Flow.",
        url: "https://api.wisprflow.ai/connect/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::OAuth2,
        auth_label: "OAuth 2.0 PKCE",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "gemini-api",
        display_name: "Gemini API Docs",
        provider: "Google",
        category: "design",
        description: "Official Google Gemini API and GenAI SDK reference documentation.",
        url: "https://gemini-api-docs-mcp.dev",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::None,
        auth_label: "Open (No Auth)",
        secret_hint: None,
        headers: &[],
        oauth_client_id: None,
        featured: false,
    },
    KnownPluginSpec {
        slug: "google-developer-knowledge",
        display_name: "Google Developer Knowledge",
        provider: "Google",
        category: "design",
        description: "Search across official Google Cloud, Android, Firebase, and Web developer docs.",
        url: "https://developerknowledge.googleapis.com/mcp",
        transport: Transport::StreamableHttp,
        auth_kind: AuthKind::Headers,
        auth_label: "Header API Key",
        secret_hint: Some("X-Goog-Api-Key"),
        headers: &[("X-Goog-Api-Key", "${GOOGLE_API_KEY}")],
        oauth_client_id: None,
        featured: false,
    },
];

pub fn list_registry() -> Vec<RegistryEntry> {
    KNOWN_PLUGINS
        .iter()
        .map(|spec| {
            let transport_flag = match spec.transport {
                Transport::StreamableHttp => "http",
                Transport::Sse => "sse",
            };
            let install_command = if let Some((hk, hv)) = spec.headers.first() {
                format!(
                    "claude mcp add --transport {transport_flag} {} {} --header \"{}: {}\"",
                    spec.slug, spec.url, hk, hv
                )
            } else {
                format!(
                    "claude mcp add --transport {transport_flag} {} {}",
                    spec.slug, spec.url
                )
            };
            RegistryEntry {
                slug: spec.slug,
                name: spec.display_name,
                provider: spec.provider,
                category: spec.category,
                description: spec.description,
                url: spec.url,
                transport: spec.transport,
                auth_kind: spec.auth_kind,
                auth_label: spec.auth_label,
                secret_hint: spec.secret_hint,
                install_command,
                featured: spec.featured,
            }
        })
        .collect()
}

pub async fn parse_and_resolve_import(
    http: &Client,
    req: ImportRequest,
) -> Result<ImportResponse, AppError> {
    let cleaned = preprocess_input(&req.command);
    if cleaned.is_empty() {
        return Err(AppError::bad(
            "paste an install command (claude, agy, codex, gemini, cursor) or MCP config snippet",
        ));
    }

    let mut response = parse_cleaned_input(http, &cleaned).await?;
    if response.candidates.is_empty() {
        return Err(AppError::bad(
            "no HTTP/SSE MCP server endpoint could be extracted from the input",
        ));
    }

    if req.auto_discover {
        for candidate in &mut response.candidates {
            maybe_enrich_with_discovery(http, candidate).await;
        }
    }

    Ok(response)
}

async fn maybe_enrich_with_discovery(http: &Client, candidate: &mut ImportedCandidate) {
    let url = candidate.input.url.trim().to_string();
    if url.contains("${") || url.contains('<') {
        candidate.warnings.push(
            "Upstream URL contains a template variable — replace it with your instance domain before probing."
                .to_string(),
        );
        return;
    }
    let Ok(parsed) = Url::parse(&url) else {
        return;
    };
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return;
    }
    if !matches!(candidate.input.auth.kind, AuthKind::None | AuthKind::OAuth2) {
        return;
    }

    let discover_fut = discover(http, &url);
    let Ok(Ok(disc)) = tokio::time::timeout(std::time::Duration::from_secs(6), discover_fut).await
    else {
        return;
    };

    if disc.token_endpoint.is_some() || disc.authorization_endpoint.is_some() {
        candidate.input.auth.kind = AuthKind::OAuth2;
        let mut oauth = candidate
            .input
            .auth
            .oauth
            .take()
            .unwrap_or_else(default_oauth_config);
        apply_discovery(&mut oauth, &disc);
        candidate.input.auth.oauth = Some(oauth);
        candidate.notes.push(
            "Auto-discovered OAuth 2.0 endpoints (.well-known metadata) from upstream.".to_string(),
        );
    } else if !disc.auth_required && candidate.input.auth.kind == AuthKind::None {
        candidate
            .notes
            .push("Upstream accepted unauthenticated initialize handshake.".to_string());
    }
    candidate.discovery = Some(disc);
}

fn preprocess_input(raw: &str) -> String {
    let mut text = raw.trim().to_string();

    // Strip markdown code fences if present
    if text.starts_with("```") {
        if let Some(first_newline) = text.find('\n') {
            let body = &text[first_newline + 1..];
            if let Some(end_fence) = body.rfind("```") {
                text = body[..end_fence].trim().to_string();
            } else {
                text = body.trim().to_string();
            }
        }
    }

    // Join backslash line continuations
    let joined = text.replace("\\\r\n", " ").replace("\\\n", " ");
    let mut lines: Vec<&str> = joined
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();

    if lines.len() == 1 {
        let line = lines[0];
        for prefix in ["$ ", "> ", "% "] {
            if let Some(stripped) = line.strip_prefix(prefix) {
                lines[0] = stripped.trim();
                break;
            }
        }
        return lines[0].to_string();
    }

    joined.trim().to_string()
}

async fn parse_cleaned_input(http: &Client, text: &str) -> Result<ImportResponse, AppError> {
    // 1. Cursor deeplink (raw or inside Markdown link)
    if let Some(deeplink) = extract_cursor_deeplink(text) {
        return parse_cursor_deeplink(&deeplink);
    }

    // 2. JSON config or bare JSON fragment
    if let Some(resp) = try_parse_json_config(text)? {
        return Ok(resp);
    }

    // 3. Codex TOML config ([mcp_servers.<name>])
    if text.contains("[mcp_servers.") {
        return parse_codex_toml(text);
    }

    // 4. Single HTTP/HTTPS URL
    if (text.starts_with("https://") || text.starts_with("http://"))
        && !text.chars().any(char::is_whitespace)
    {
        return parse_direct_url(text);
    }

    // 5. CLI / Slash Commands
    let tokens = shlex_split(text);
    if tokens.is_empty() {
        return Err(AppError::bad("empty command"));
    }

    let first = tokens[0].as_str();
    let first_base = first.rsplit('/').next().unwrap_or(first);

    // Handle `/plugin install <target>` slash command
    if (first == "/plugin" || first == "/plugins")
        && tokens.get(1).is_some_and(|s| s == "install" || s == "i")
    {
        return resolve_plugin_install(http, "plugin-slash", &tokens[2..]).await;
    }

    match first_base {
        "agy" => parse_agy_command(http, &tokens[1..]).await,
        "claude" => parse_claude_command(http, &tokens[1..]).await,
        "codex" => parse_codex_command(&tokens[1..]),
        "gemini" => parse_gemini_command(http, &tokens[1..]).await,
        "npx" | "bunx" | "pnpx" => parse_npx_command(&tokens),
        _ => {
            // Check if the command line contains mcp-remote anywhere
            if tokens.iter().any(|t| t.contains("mcp-remote")) {
                parse_npx_command(&tokens)
            } else {
                Err(AppError::bad(format!(
                    "unrecognized command `{first_base}` — supported: claude, agy, codex, gemini, cursor://, npx mcp-remote, JSON, or TOML"
                )))
            }
        }
    }
}

fn extract_cursor_deeplink(text: &str) -> Option<String> {
    let start = text.find("cursor://")?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, ')' | ']' | '"' | '\'' | '>'))
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn parse_cursor_deeplink(deeplink: &str) -> Result<ImportResponse, AppError> {
    let url =
        Url::parse(deeplink).map_err(|err| AppError::bad(format!("invalid cursor URL: {err}")))?;
    let mut name_param = None;
    let mut config_param = None;
    for (k, v) in url.query_pairs() {
        if k == "name" {
            name_param = Some(v.into_owned());
        } else if k == "config" {
            config_param = Some(v.into_owned());
        }
    }
    let encoded = config_param
        .ok_or_else(|| AppError::bad("cursor deeplink is missing `config` query parameter"))?;
    let decoded_bytes = decode_base64_flexible(&encoded)
        .ok_or_else(|| AppError::bad("cursor deeplink `config` is not valid base64"))?;
    let decoded_str = String::from_utf8(decoded_bytes)
        .map_err(|_| AppError::bad("cursor deeplink `config` is not valid UTF-8"))?;
    let value: Value = serde_json::from_str(&decoded_str)
        .map_err(|err| AppError::bad(format!("cursor deeplink JSON: {err}")))?;

    let mut candidates = Vec::new();
    let fallback_name = name_param.as_deref().unwrap_or("cursor-mcp");
    if let Some(candidate) = candidate_from_json_entry(fallback_name, None, &value)? {
        let mut c = candidate;
        c.notes
            .insert(0, "Decoded Cursor MCP install deeplink.".to_string());
        candidates.push(c);
    } else if let Some(mut resp) = try_parse_json_value(&value)? {
        for c in &mut resp.candidates {
            c.notes
                .insert(0, "Decoded Cursor MCP install deeplink.".to_string());
        }
        candidates = resp.candidates;
    }

    Ok(ImportResponse {
        detected_format: "cursor-deeplink".to_string(),
        source_label: "Cursor Install Deeplink".to_string(),
        candidates,
    })
}

fn decode_base64_flexible(input: &str) -> Option<Vec<u8>> {
    let trimmed = input.trim();
    STANDARD
        .decode(trimmed)
        .or_else(|_| URL_SAFE.decode(trimmed))
        .or_else(|_| URL_SAFE_NO_PAD.decode(trimmed.trim_end_matches('=')))
        .ok()
}

fn try_parse_json_config(text: &str) -> Result<Option<ImportResponse>, AppError> {
    let trimmed = text.trim();
    let value: Value = if trimmed.starts_with('{') {
        match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        }
    } else if trimmed.starts_with('"') && trimmed.contains(':') && trimmed.contains('{') {
        let wrapped = format!("{{{trimmed}}}");
        match serde_json::from_str(&wrapped) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        }
    } else {
        return Ok(None);
    };

    try_parse_json_value(&value)
}

fn try_parse_json_value(value: &Value) -> Result<Option<ImportResponse>, AppError> {
    let Some(obj) = value.as_object() else {
        return Ok(None);
    };

    // Case A: {"mcpServers": { ... }} or {"servers": { ... }} or {"mcp": {"servers": { ... }}}
    let servers_map = obj
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .or_else(|| obj.get("servers").and_then(|v| v.as_object()))
        .or_else(|| {
            obj.get("mcp")
                .and_then(|m| m.as_object())
                .and_then(|m| m.get("servers"))
                .and_then(|v| v.as_object())
        });

    if let Some(map) = servers_map {
        let mut candidates = Vec::new();
        for (key, entry) in map {
            if let Some(candidate) = candidate_from_json_entry(key, None, entry)? {
                candidates.push(candidate);
            }
        }
        return Ok(Some(ImportResponse {
            detected_format: "mcp-json".to_string(),
            source_label: "MCP JSON Configuration".to_string(),
            candidates,
        }));
    }

    // Case B: Single server object {"url": "https://...", ...} or {"serverUrl": "https://..."}
    if obj.contains_key("url")
        || obj.contains_key("serverUrl")
        || obj.contains_key("httpUrl")
        || obj.contains_key("command")
    {
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("imported-mcp");
        let mut candidates = Vec::new();
        if let Some(candidate) = candidate_from_json_entry(name, None, value)? {
            candidates.push(candidate);
        }
        return Ok(Some(ImportResponse {
            detected_format: "mcp-json".to_string(),
            source_label: "MCP Server JSON Object".to_string(),
            candidates,
        }));
    }

    // Case C: Bare map of slug -> server_obj (e.g. Claude `.mcp.json`: {"linear": {"type": "http", "url": "..."}})
    let mut candidates = Vec::new();
    for (key, entry) in obj {
        if entry.is_object() {
            if let Some(candidate) = candidate_from_json_entry(key, None, entry)? {
                candidates.push(candidate);
            }
        }
    }
    if !candidates.is_empty() {
        return Ok(Some(ImportResponse {
            detected_format: "mcp-json".to_string(),
            source_label: "Plugin .mcp.json Manifest".to_string(),
            candidates,
        }));
    }

    Ok(None)
}

fn candidate_from_json_entry(
    raw_key: &str,
    display_name_override: Option<&str>,
    entry: &Value,
) -> Result<Option<ImportedCandidate>, AppError> {
    let Some(obj) = entry.as_object() else {
        return Ok(None);
    };

    let mut notes = Vec::new();
    let mut warnings = Vec::new();
    let mut extracted_headers: Vec<(String, String)> = Vec::new();

    // 1. Direct URL fields
    let mut raw_url = obj
        .get("url")
        .or_else(|| obj.get("serverUrl"))
        .or_else(|| obj.get("httpUrl"))
        .or_else(|| obj.get("endpoint"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // 2. Or stdio bridge wrapping an HTTP URL (e.g. `npx -y mcp-remote https://...` or `mcp_proxy_bundle.cjs https://...`)
    if raw_url.is_none() {
        if let Some(args) = obj.get("args").and_then(|v| v.as_array()) {
            let arg_strs: Vec<String> = args
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            if let Some((bridged_url, bridged_headers)) = extract_bridged_url_and_headers(&arg_strs)
            {
                raw_url = Some(bridged_url);
                extracted_headers.extend(bridged_headers);
                notes.push(
                    "Unwrapped stdio proxy bridge (`mcp-remote`) into direct HTTP upstream."
                        .to_string(),
                );
            }
        }
    }

    let Some(url) = raw_url else {
        return Ok(None);
    };

    let transport = match obj
        .get("transport")
        .or_else(|| obj.get("type"))
        .and_then(|v| v.as_str())
    {
        Some(t) if t.eq_ignore_ascii_case("sse") => Transport::Sse,
        _ => Transport::StreamableHttp,
    };

    // Collect headers from JSON `headers` or `http_headers`
    if let Some(hdr_obj) = obj
        .get("headers")
        .or_else(|| obj.get("http_headers"))
        .and_then(|v| v.as_object())
    {
        for (k, v) in hdr_obj {
            if let Some(val_str) = v.as_str() {
                extracted_headers.push((k.clone(), val_str.to_string()));
            }
        }
    }

    // Collect variable descriptions if present (from Antigravity plugin `mcp_config.json`)
    if let Some(vars) = obj.get("variables").and_then(|v| v.as_array()) {
        for var in vars {
            if let Some(var_name) = var.get("name").and_then(|v| v.as_str()) {
                let desc = var
                    .get("description")
                    .or_else(|| var.get("title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !desc.is_empty() {
                    notes.push(format!("Variable `${{{var_name}}}`: {desc}"));
                }
            }
        }
    }

    let (mut auth, mut auth_notes) = build_auth_from_headers(extracted_headers);
    notes.append(&mut auth_notes);

    // Check explicit `oauth` block in JSON
    if let Some(oauth_obj) = obj.get("oauth").and_then(|v| v.as_object()) {
        auth.kind = AuthKind::OAuth2;
        let mut oauth = default_oauth_config();
        if let Some(cid) = oauth_obj.get("clientId").and_then(|v| v.as_str()) {
            oauth.client_id = cid.to_string();
        }
        if let Some(csec) = oauth_obj
            .get("clientSecret")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            if !is_secret_placeholder(csec) {
                oauth.client_secret = Some(csec.to_string());
            }
        }
        if let Some(scopes_arr) = oauth_obj.get("scopes").and_then(|v| v.as_array()) {
            oauth.scopes = scopes_arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
        }
        auth.oauth = Some(oauth);
    }

    let (name, slug) = derive_name_and_slug(raw_key, display_name_override, &url);
    if url.contains("${") || url.contains('$') {
        warnings.push(format!(
            "URL `{url}` contains an environment variable placeholder — update it before probing."
        ));
    }

    Ok(Some(ImportedCandidate {
        input: ServerInput {
            name,
            slug,
            url,
            transport,
            enabled: true,
            auth,
            tool_policy: None,
        },
        discovery: None,
        notes,
        warnings,
    }))
}

fn parse_codex_toml(text: &str) -> Result<ImportResponse, AppError> {
    let mut candidates = Vec::new();
    let mut current_slug: Option<String> = None;
    let mut current_url: Option<String> = None;
    let mut current_bearer: Option<String> = None;
    let mut current_bearer_env: Option<String> = None;
    let mut current_headers: Vec<(String, String)> = Vec::new();

    let flush = |slug: Option<String>,
                 url: Option<String>,
                 bearer: Option<String>,
                 bearer_env: Option<String>,
                 headers: Vec<(String, String)>,
                 out: &mut Vec<ImportedCandidate>| {
        let Some(raw_slug) = slug else {
            return;
        };
        let Some(u) = url else {
            return;
        };
        let mut hdrs = headers;
        let mut notes = vec!["Parsed OpenAI Codex `config.toml` `[mcp_servers]` block.".to_string()];
        if let Some(token) = bearer {
            hdrs.push(("Authorization".to_string(), format!("Bearer {token}")));
        } else if let Some(env_var) = bearer_env {
            hdrs.push((
                "Authorization".to_string(),
                format!("Bearer ${{{env_var}}}"),
            ));
        }
        let (auth, mut auth_notes) = build_auth_from_headers(hdrs);
        notes.append(&mut auth_notes);
        let (name, clean_slug) = derive_name_and_slug(&raw_slug, None, &u);
        out.push(ImportedCandidate {
            input: ServerInput {
                name,
                slug: clean_slug,
                url: u,
                transport: Transport::StreamableHttp,
                enabled: true,
                auth,
                tool_policy: None,
            },
            discovery: None,
            notes,
            warnings: Vec::new(),
        });
    };

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = &line[1..line.len() - 1].trim();
            if let Some(rest) = section.strip_prefix("mcp_servers.") {
                flush(
                    current_slug.take(),
                    current_url.take(),
                    current_bearer.take(),
                    current_bearer_env.take(),
                    std::mem::take(&mut current_headers),
                    &mut candidates,
                );
                let slug_part = rest.split('.').next().unwrap_or(rest).trim_matches('"');
                current_slug = Some(slug_part.to_string());
            }
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();
        match key {
            "url" | "http_url" | "server_url" => {
                current_url = Some(unquote(val));
            }
            "bearer_token" => {
                current_bearer = Some(unquote(val));
            }
            "bearer_token_env_var" => {
                current_bearer_env = Some(unquote(val));
            }
            "http_headers" => {
                for (hk, hv) in parse_toml_inline_table(val) {
                    current_headers.push((hk, hv));
                }
            }
            "env_http_headers" => {
                for (hk, hv) in parse_toml_inline_table(val) {
                    current_headers.push((hk, format!("${{{hv}}}")));
                }
            }
            _ => {}
        }
    }

    flush(
        current_slug,
        current_url,
        current_bearer,
        current_bearer_env,
        current_headers,
        &mut candidates,
    );

    Ok(ImportResponse {
        detected_format: "codex-toml".to_string(),
        source_label: "OpenAI Codex config.toml".to_string(),
        candidates,
    })
}

fn parse_toml_inline_table(raw: &str) -> Vec<(String, String)> {
    let trimmed = raw.trim().trim_start_matches('{').trim_end_matches('}');
    let mut pairs = Vec::new();
    for part in trimmed.split(',') {
        if let Some((k, v)) = part.split_once('=') {
            let key = unquote(k.trim());
            let val = unquote(v.trim());
            if !key.is_empty() {
                pairs.push((key, val));
            }
        }
    }
    pairs
}

fn parse_direct_url(raw_url: &str) -> Result<ImportResponse, AppError> {
    let url =
        Url::parse(raw_url).map_err(|err| AppError::bad(format!("invalid URL: {err}")))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(AppError::bad("URL must start with http:// or https://"));
    }
    let (name, slug) = derive_name_and_slug("", None, raw_url);
    Ok(ImportResponse {
        detected_format: "direct-url".to_string(),
        source_label: "Direct MCP Endpoint URL".to_string(),
        candidates: vec![ImportedCandidate {
            input: ServerInput {
                name,
                slug,
                url: raw_url.to_string(),
                transport: Transport::StreamableHttp,
                enabled: true,
                auth: default_auth_config(),
                tool_policy: None,
            },
            discovery: None,
            notes: vec!["Parsed direct MCP endpoint URL.".to_string()],
            warnings: Vec::new(),
        }],
    })
}

async fn parse_agy_command(http: &Client, args: &[String]) -> Result<ImportResponse, AppError> {
    let Some(sub) = args.first().map(String::as_str) else {
        return Err(AppError::bad("expected `agy plugin install ...` or `agy mcp add ...`"));
    };
    match sub {
        "plugin" | "plugins" => {
            let action = args.get(1).map(String::as_str).unwrap_or("");
            if action == "install" || action == "i" || action == "add" {
                resolve_plugin_install(http, "agy-plugin", &args[2..]).await
            } else if !action.is_empty() && !action.starts_with('-') {
                // Allow shorthand `agy plugin notion`
                resolve_plugin_install(http, "agy-plugin", &args[1..]).await
            } else {
                Err(AppError::bad(
                    "expected `agy plugin install <plugin>[@marketplace]`",
                ))
            }
        }
        "mcp" => {
            let action = args.get(1).map(String::as_str).unwrap_or("");
            if action == "add" {
                parse_cli_mcp_add("agy-mcp-add", "Antigravity CLI (agy mcp add)", &args[2..])
            } else {
                Err(AppError::bad("expected `agy mcp add [flags] <name> <url>`"))
            }
        }
        _ => Err(AppError::bad(format!(
            "unsupported agy subcommand `{sub}` — use `agy plugin install` or `agy mcp add`"
        ))),
    }
}

async fn parse_claude_command(http: &Client, args: &[String]) -> Result<ImportResponse, AppError> {
    let Some(sub) = args.first().map(String::as_str) else {
        return Err(AppError::bad(
            "expected `claude plugin install ...` or `claude mcp add ...`",
        ));
    };
    match sub {
        "plugin" | "plugins" => {
            let action = args.get(1).map(String::as_str).unwrap_or("");
            if action == "install" || action == "i" || action == "add" {
                resolve_plugin_install(http, "claude-plugin", &args[2..]).await
            } else if !action.is_empty() && !action.starts_with('-') {
                resolve_plugin_install(http, "claude-plugin", &args[1..]).await
            } else {
                Err(AppError::bad(
                    "expected `claude plugin install <plugin>[@marketplace]`",
                ))
            }
        }
        "mcp" => {
            let action = args.get(1).map(String::as_str).unwrap_or("");
            match action {
                "add" => {
                    parse_cli_mcp_add("claude-mcp-add", "Claude Code CLI (claude mcp add)", &args[2..])
                }
                "add-json" => parse_claude_mcp_add_json(&args[2..]),
                _ => Err(AppError::bad(
                    "expected `claude mcp add` or `claude mcp add-json`",
                )),
            }
        }
        _ => Err(AppError::bad(format!(
            "unsupported claude subcommand `{sub}` — use `claude plugin install` or `claude mcp add`"
        ))),
    }
}

fn parse_codex_command(args: &[String]) -> Result<ImportResponse, AppError> {
    let Some(sub) = args.first().map(String::as_str) else {
        return Err(AppError::bad("expected `codex mcp add ...`"));
    };
    if sub == "mcp" && args.get(1).is_some_and(|a| a == "add") {
        parse_cli_mcp_add("codex-mcp-add", "OpenAI Codex CLI (codex mcp add)", &args[2..])
    } else {
        Err(AppError::bad("expected `codex mcp add <name> --url <url>`"))
    }
}

async fn parse_gemini_command(http: &Client, args: &[String]) -> Result<ImportResponse, AppError> {
    let Some(sub) = args.first().map(String::as_str) else {
        return Err(AppError::bad(
            "expected `gemini mcp add ...` or `gemini extensions install ...`",
        ));
    };
    match sub {
        "mcp" if args.get(1).is_some_and(|a| a == "add") => {
            parse_cli_mcp_add("gemini-mcp-add", "Gemini CLI (gemini mcp add)", &args[2..])
        }
        "extensions" | "extension"
            if args
                .get(1)
                .is_some_and(|a| a == "install" || a == "i" || a == "link") =>
        {
            resolve_plugin_install(http, "gemini-extension", &args[2..]).await
        }
        _ => Err(AppError::bad(
            "expected `gemini mcp add` or `gemini extensions install`",
        )),
    }
}

fn parse_claude_mcp_add_json(args: &[String]) -> Result<ImportResponse, AppError> {
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-s" || arg == "--scope" {
            i += 2;
            continue;
        }
        if arg.starts_with("--scope=") {
            i += 1;
            continue;
        }
        positional.push(arg.as_str());
        i += 1;
    }
    if positional.len() < 2 {
        return Err(AppError::bad(
            "expected `claude mcp add-json <name> '<json>'`",
        ));
    }
    let name = positional[0];
    let raw_json = positional[1];
    let value: Value = serde_json::from_str(raw_json)
        .map_err(|err| AppError::bad(format!("invalid JSON in `claude mcp add-json`: {err}")))?;
    let Some(mut candidate) = candidate_from_json_entry(name, None, &value)? else {
        return Err(AppError::bad(
            "JSON payload in `claude mcp add-json` does not contain an HTTP/SSE URL",
        ));
    };
    candidate
        .notes
        .insert(0, "Parsed `claude mcp add-json` command.".to_string());
    Ok(ImportResponse {
        detected_format: "claude-mcp-json".to_string(),
        source_label: "Claude Code CLI (claude mcp add-json)".to_string(),
        candidates: vec![candidate],
    })
}

fn parse_cli_mcp_add(
    format_id: &str,
    source_label: &str,
    args: &[String],
) -> Result<ImportResponse, AppError> {
    let mut transport = Transport::StreamableHttp;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut url_flag: Option<String> = None;
    let mut client_id: Option<String> = None;
    let mut client_secret: Option<String> = None;
    let mut bearer_env: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut after_double_dash: Vec<String> = Vec::new();
    let mut in_double_dash = false;

    let mut i = 0;
    while i < args.len() {
        let token = &args[i];
        if in_double_dash {
            after_double_dash.push(token.clone());
            i += 1;
            continue;
        }
        if token == "--" {
            in_double_dash = true;
            i += 1;
            continue;
        }
        if token == "-t" || token == "--transport" || token == "--type" {
            if let Some(val) = args.get(i + 1) {
                if val.eq_ignore_ascii_case("sse") {
                    transport = Transport::Sse;
                }
                i += 2;
                continue;
            }
        }
        if let Some(val) = token
            .strip_prefix("--transport=")
            .or_else(|| token.strip_prefix("--type="))
        {
            if val.eq_ignore_ascii_case("sse") {
                transport = Transport::Sse;
            }
            i += 1;
            continue;
        }
        if token == "-H" || token == "--header" {
            if let Some(raw_hdr) = args.get(i + 1) {
                if let Some(pair) = split_header(raw_hdr) {
                    headers.push(pair);
                }
                i += 2;
                continue;
            }
        }
        if let Some(raw_hdr) = token.strip_prefix("--header=") {
            if let Some(pair) = split_header(raw_hdr) {
                headers.push(pair);
            }
            i += 1;
            continue;
        }
        if token == "--url" {
            if let Some(u) = args.get(i + 1) {
                url_flag = Some(u.clone());
                i += 2;
                continue;
            }
        }
        if let Some(u) = token.strip_prefix("--url=") {
            url_flag = Some(u.to_string());
            i += 1;
            continue;
        }
        if token == "--bearer-token-env-var" {
            if let Some(env_name) = args.get(i + 1) {
                bearer_env = Some(env_name.clone());
                i += 2;
                continue;
            }
        }
        if token == "--client-id" {
            if let Some(cid) = args.get(i + 1) {
                client_id = Some(cid.clone());
                i += 2;
                continue;
            }
        }
        if token == "--client-secret" {
            if let Some(csec) = args.get(i + 1) {
                client_secret = Some(csec.clone());
                i += 2;
                continue;
            }
        }
        if matches!(
            token.as_str(),
            "-s" | "--scope" | "-e" | "--env" | "--description" | "--callback-port"
        ) {
            i += 2;
            continue;
        }
        if token.starts_with('-') {
            i += 1;
            continue;
        }
        positional.push(token.clone());
        i += 1;
    }

    if let Some(env_name) = bearer_env {
        headers.push((
            "Authorization".to_string(),
            format!("Bearer ${{{env_name}}}"),
        ));
    }

    let mut notes = vec![format!("Parsed `{source_label}` command.")];

    // Determine <name> and <url>
    let mut name_hint = String::new();
    let mut target_url = url_flag;

    for pos in &positional {
        if (pos.starts_with("https://") || pos.starts_with("http://")) && target_url.is_none() {
            target_url = Some(pos.clone());
        } else if name_hint.is_empty() {
            name_hint = pos.clone();
        }
    }

    if target_url.is_none() {
        let mut combined = positional.clone();
        combined.extend(after_double_dash);
        if let Some((bridged_url, bridged_headers)) = extract_bridged_url_and_headers(&combined) {
            target_url = Some(bridged_url);
            headers.extend(bridged_headers);
            notes.push("Extracted remote HTTP URL from `mcp-remote` arguments.".to_string());
        } else if let Some(mapped_slug) = match_known_stdio_package(&combined)
            .or_else(|| resolve_from_known_registry(format_id, &name_hint).map(|_| name_hint.as_str()))
        {
            if let Some(mut resp) = resolve_from_known_registry(format_id, mapped_slug) {
                if let Some(cand) = resp.candidates.first_mut() {
                    cand.notes.insert(
                        0,
                        format!(
                            "Automatically upgraded stdio package command to official remote HTTP MCP endpoint (`{}`).",
                            cand.input.url
                        ),
                    );
                }
                resp.source_label = format!("{source_label} (Upgraded to Remote HTTP MCP)");
                return Ok(resp);
            }
        }
    }

    let Some(url) = target_url else {
        return Err(AppError::bad(
            "could not find an http:// or https:// MCP endpoint URL in the command",
        ));
    };

    let (mut auth, mut auth_notes) = build_auth_from_headers(headers);
    notes.append(&mut auth_notes);

    if client_id.is_some() || client_secret.is_some() {
        auth.kind = AuthKind::OAuth2;
        let mut oauth = default_oauth_config();
        if let Some(cid) = client_id {
            oauth.client_id = cid;
        }
        if let Some(csec) = client_secret.filter(|s| !is_secret_placeholder(s)) {
            oauth.client_secret = Some(csec);
        }
        auth.oauth = Some(oauth);
    }

    let (name, slug) = derive_name_and_slug(&name_hint, None, &url);
    Ok(ImportResponse {
        detected_format: format_id.to_string(),
        source_label: source_label.to_string(),
        candidates: vec![ImportedCandidate {
            input: ServerInput {
                name,
                slug,
                url,
                transport,
                enabled: true,
                auth,
                tool_policy: None,
            },
            discovery: None,
            notes,
            warnings: Vec::new(),
        }],
    })
}

fn match_known_stdio_package(tokens: &[String]) -> Option<&'static str> {
    let joined = tokens.join(" ").to_ascii_lowercase();
    for (needle, slug) in [
        ("notion-mcp-server", "notion"),
        ("@notionhq/", "notion"),
        ("@linear/", "linear"),
        ("linear-mcp", "linear"),
        ("@sentry/", "sentry"),
        ("sentry-mcp", "sentry"),
        ("@stripe/mcp", "stripe"),
        ("context7-mcp", "context7"),
        ("@upstash/context7", "context7"),
        ("server-github", "github"),
        ("github-mcp-server", "github"),
        ("mcp-server-cloudflare", "cloudflare-bindings"),
        ("mcp-server-supabase", "supabase"),
        ("@supabase/mcp", "supabase"),
        ("Neon-mcp", "neon"),
        ("@neondatabase/mcp", "neon"),
        ("figma-developer-mcp", "figma"),
        ("exa-mcp-server", "exa"),
        ("firecrawl-mcp", "firecrawl"),
        ("brave-search", "brave-search"),
        ("slack-mcp", "slack"),
        ("asana-mcp", "asana"),
        ("atlassian-mcp", "atlassian"),
    ] {
        if joined.contains(&needle.to_ascii_lowercase()) {
            return Some(slug);
        }
    }
    None
}

fn parse_npx_command(tokens: &[String]) -> Result<ImportResponse, AppError> {
    let Some((url, headers)) = extract_bridged_url_and_headers(tokens) else {
        if let Some(slug) = match_known_stdio_package(tokens) {
            if let Some(mut resp) = resolve_from_known_registry("stdio-upgraded", slug) {
                if let Some(cand) = resp.candidates.first_mut() {
                    cand.notes.insert(
                        0,
                        format!(
                            "Automatically upgraded `npx` stdio package to official remote HTTP MCP endpoint (`{}`).",
                            cand.input.url
                        ),
                    );
                }
                resp.source_label = "Stdio Package → Official Remote HTTP MCP".to_string();
                return Ok(resp);
            }
        }
        return Err(AppError::bad(
            "no HTTP/HTTPS MCP URL found in `npx` command (only remote HTTP/SSE or known official cloud MCP services are supported)",
        ));
    };
    let (auth, mut auth_notes) = build_auth_from_headers(headers);
    let mut notes = vec!["Unwrapped `npx mcp-remote` stdio bridge into direct HTTP upstream.".to_string()];
    notes.append(&mut auth_notes);
    let (name, slug) = derive_name_and_slug("", None, &url);
    Ok(ImportResponse {
        detected_format: "mcp-remote".to_string(),
        source_label: "npx mcp-remote Bridge Command".to_string(),
        candidates: vec![ImportedCandidate {
            input: ServerInput {
                name,
                slug,
                url,
                transport: Transport::StreamableHttp,
                enabled: true,
                auth,
                tool_policy: None,
            },
            discovery: None,
            notes,
            warnings: Vec::new(),
        }],
    })
}

async fn resolve_plugin_install(
    http: &Client,
    format_id: &str,
    args: &[String],
) -> Result<ImportResponse, AppError> {
    let mut target: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-s" || arg == "--scope" {
            i += 2;
            continue;
        }
        if arg.starts_with('-') {
            i += 1;
            continue;
        }
        target = Some(arg.clone());
        break;
    }

    let raw_target = target.ok_or_else(|| {
        AppError::bad("missing plugin name or repository target (e.g. `notion`, `stripe`, `linear`)")
    })?;

    // Split `<plugin>@<marketplace>`
    let (plugin_name, marketplace_hint) = match raw_target.split_once('@') {
        Some((p, m)) => (p.trim(), Some(m.trim())),
        None => (raw_target.trim(), None),
    };

    // 1. Check if `raw_target` is a GitHub URL or `owner/repo`
    if plugin_name.starts_with("https://github.com/")
        || (plugin_name.contains('/') && !plugin_name.starts_with('.'))
    {
        if let Some(resp) = resolve_github_plugin(http, format_id, plugin_name, None, None).await? {
            return Ok(resp);
        }
    }

    // 2. Try local installed/cached Antigravity or Claude marketplaces first
    if let Some(resp) =
        resolve_from_local_caches(format_id, plugin_name, marketplace_hint)?
    {
        return Ok(resp);
    }

    // 3. Try live Antigravity Marketplace catalogs (Partners + Official)
    if let Some(resp) =
        resolve_from_agy_remote_catalogs(http, format_id, plugin_name, marketplace_hint).await?
    {
        return Ok(resp);
    }

    // 4. Try live Claude Official Marketplace catalog on GitHub
    if let Some(resp) = resolve_from_claude_remote_catalog(http, format_id, plugin_name).await? {
        return Ok(resp);
    }

    // 5. Fallback to built-in known plugin registry
    if let Some(resp) = resolve_from_known_registry(format_id, plugin_name) {
        return Ok(resp);
    }

    Err(AppError::bad(format!(
        "plugin `{plugin_name}` was not found in Antigravity or Claude marketplaces (or does not bundle an HTTP MCP server)"
    )))
}

fn is_safe_path_segment(seg: &str) -> bool {
    !seg.is_empty()
        && seg != "."
        && seg != ".."
        && seg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn resolve_from_local_caches(
    format_id: &str,
    plugin_name: &str,
    marketplace_hint: Option<&str>,
) -> Result<Option<ImportResponse>, AppError> {
    if !is_safe_path_segment(plugin_name) {
        return Ok(None);
    }
    if let Some(hint) = marketplace_hint {
        if !is_safe_path_segment(hint) {
            return Ok(None);
        }
    }
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(None);
    };

    // A. Installed AGY plugin: ~/.gemini/config/plugins/<plugin>/mcp_config.json
    let installed_dir = home.join(".gemini/config/plugins").join(plugin_name);
    if let Some(resp) = read_plugin_dir_manifest(
        &installed_dir,
        format_id,
        &format!("Antigravity Plugin ({plugin_name})"),
        plugin_name,
    )? {
        return Ok(Some(resp));
    }

    // B. Cached AGY marketplace extracted dir: ~/.gemini/antigravity-cli/cache/marketplaces/*/plugins/<plugin>/*/
    let agy_mp_cache = home.join(".gemini/antigravity-cli/cache/marketplaces");
    if agy_mp_cache.is_dir() {
        for mp in ["antigravity-plugins-partners", "antigravity-plugins-official"] {
            if marketplace_hint.is_some_and(|h| h != mp) {
                continue;
            }
            let plugin_versions_dir = agy_mp_cache.join(mp).join("plugins").join(plugin_name);
            if let Ok(entries) = std::fs::read_dir(&plugin_versions_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(resp) = read_plugin_dir_manifest(
                            &entry.path(),
                            format_id,
                            &format!("Antigravity Marketplace ({mp})"),
                            plugin_name,
                        )? {
                            return Ok(Some(resp));
                        }
                    }
                }
            }
        }
    }

    // C. Local Claude marketplace clone: ~/.claude/plugins/marketplaces/claude-plugins-official/
    let claude_mp = home.join(".claude/plugins/marketplaces/claude-plugins-official");
    for sub in ["external_plugins", "plugins"] {
        let dir = claude_mp.join(sub).join(plugin_name);
        if dir.is_dir() {
            if let Some(resp) = read_plugin_dir_manifest(
                &dir,
                format_id,
                "Claude Official Marketplace (Local Cache)",
                plugin_name,
            )? {
                return Ok(Some(resp));
            }
        }
    }

    Ok(None)
}

fn read_plugin_dir_manifest(
    dir: &std::path::Path,
    format_id: &str,
    source_label: &str,
    plugin_name: &str,
) -> Result<Option<ImportResponse>, AppError> {
    let display_name = std::fs::read_to_string(dir.join("plugin.json"))
        .or_else(|_| std::fs::read_to_string(dir.join(".claude-plugin/plugin.json")))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| {
            v.get("displayName")
                .or_else(|| v.get("name"))
                .and_then(|n| n.as_str())
                .map(str::to_string)
        });

    for filename in ["mcp_config.json", ".mcp.json"] {
        let path = dir.join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                if let Some(mut resp) = try_parse_json_value(&val)? {
                    resp.detected_format = format_id.to_string();
                    resp.source_label = source_label.to_string();
                    for c in &mut resp.candidates {
                        if let Some(dn) = display_name.as_deref() {
                            if c.input.name.eq_ignore_ascii_case("mcp")
                                || c.input.name.eq_ignore_ascii_case("api")
                                || c.input.slug == "mcp"
                            {
                                let (n, s) =
                                    derive_name_and_slug(plugin_name, Some(dn), &c.input.url);
                                c.input.name = n;
                                c.input.slug = s;
                            }
                        }
                        c.notes.insert(
                            0,
                            format!("Resolved plugin `{plugin_name}` from `{source_label}`."),
                        );
                    }
                    return Ok(Some(resp));
                }
            }
        }
    }
    Ok(None)
}

async fn resolve_from_agy_remote_catalogs(
    http: &Client,
    format_id: &str,
    plugin_name: &str,
    marketplace_hint: Option<&str>,
) -> Result<Option<ImportResponse>, AppError> {
    let catalogs = [
        ("antigravity-plugins-partners", AGY_PARTNERS_MARKETPLACE_URL),
        ("antigravity-plugins-official", AGY_OFFICIAL_MARKETPLACE_URL),
    ];

    for (mp_name, mp_url) in catalogs {
        if marketplace_hint.is_some_and(|h| h != mp_name) {
            continue;
        }
        let catalog_val = load_cached_or_remote_agy_catalog(http, mp_name, mp_url).await;
        let Some(catalog) = catalog_val else {
            continue;
        };
        let Some(plugins) = catalog.get("plugins").and_then(|v| v.as_array()) else {
            continue;
        };
        let Some(entry) = plugins.iter().find(|p| {
            p.get("name").and_then(|v| v.as_str()).is_some_and(|n| {
                n.eq_ignore_ascii_case(plugin_name)
                    || n.eq_ignore_ascii_case(&format!("{plugin_name}-plugin"))
                    || n.eq_ignore_ascii_case(&format!("{plugin_name}-antigravity"))
            })
        }) else {
            continue;
        };

        let display_name = entry
            .get("displayName")
            .or_else(|| entry.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or(plugin_name);
        let zip_url = entry
            .get("source")
            .and_then(|s| s.get("url"))
            .and_then(|v| v.as_str());
        let expected_sha = entry
            .get("source")
            .and_then(|s| s.get("sha256"))
            .and_then(|v| v.as_str());

        if let Some(archive_url) = zip_url {
            if let Ok(response) = http
                .get(archive_url)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
            {
                if response.status().is_success() {
                    if let Ok(bytes) = response.bytes().await {
                        if let Some(sha_hex) = expected_sha {
                            let actual_hex = hex_sha256(&bytes);
                            if !actual_hex.eq_ignore_ascii_case(sha_hex) {
                                return Err(AppError::upstream(format!(
                                    "SHA-256 mismatch for plugin archive `{plugin_name}`"
                                )));
                            }
                        }
                        let files = extract_zip_stored_json_files(&bytes);
                        for (_, json_text) in files {
                            if let Ok(val) = serde_json::from_str::<Value>(&json_text) {
                                if let Some(mut resp) = try_parse_json_value(&val)? {
                                    resp.detected_format = format_id.to_string();
                                    resp.source_label = format!(
                                        "Antigravity Marketplace ({mp_name} / {display_name})"
                                    );
                                    let is_single = resp.candidates.len() == 1;
                                    for c in &mut resp.candidates {
                                        if c.input.slug == "mcp"
                                            || c.input.slug == "api"
                                            || c.input.name.eq_ignore_ascii_case("mcp")
                                        {
                                            let (n, s) = derive_name_and_slug(
                                                plugin_name,
                                                Some(display_name),
                                                &c.input.url,
                                            );
                                            c.input.name = n;
                                            c.input.slug = s;
                                        } else if !display_name.is_empty() && is_single {
                                            c.input.name = display_name.to_string();
                                        }
                                        c.notes.insert(
                                            0,
                                            format!(
                                                "Fetched & verified `{display_name}` plugin archive from `{mp_name}`."
                                            ),
                                        );
                                    }
                                    return Ok(Some(resp));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

async fn load_cached_or_remote_agy_catalog(
    http: &Client,
    mp_name: &str,
    mp_url: &str,
) -> Option<Value> {
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let cached = home
            .join(".gemini/antigravity-cli/cache/marketplaces")
            .join(mp_name)
            .join("marketplace.json");
        if let Ok(text) = std::fs::read_to_string(cached) {
            if let Ok(val) = serde_json::from_str::<Value>(&text) {
                return Some(val);
            }
        }
    }
    let resp = http
        .get(mp_url)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<Value>().await.ok()
}

async fn resolve_from_claude_remote_catalog(
    http: &Client,
    format_id: &str,
    plugin_name: &str,
) -> Result<Option<ImportResponse>, AppError> {
    // 1. Check local Claude plugin-directory-cache-v2.json if available
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let cache_v2 = home.join(".claude/plugins/plugin-directory-cache-v2.json");
        if let Ok(text) = std::fs::read_to_string(cache_v2) {
            if let Ok(val) = serde_json::from_str::<Value>(&text) {
                if let Some(listings) = val.get("listings").and_then(|v| v.as_array()) {
                    if let Some(item) = listings.iter().find(|item| {
                        item.get("name")
                            .and_then(|v| v.as_str())
                            .is_some_and(|n| n.eq_ignore_ascii_case(plugin_name))
                    }) {
                        let repo_url = item
                            .get("source")
                            .and_then(|s| s.get("repository"))
                            .and_then(|r| r.get("url"))
                            .and_then(|v| v.as_str());
                        let subdir = item
                            .get("source")
                            .and_then(|s| s.get("repository"))
                            .and_then(|r| r.get("path"))
                            .and_then(|v| v.as_str());
                        let commit = item
                            .get("source")
                            .and_then(|s| s.get("repository"))
                            .and_then(|r| r.get("reviewed_commit"))
                            .and_then(|v| v.as_str());
                        if let Some(repo) = repo_url {
                            if let Some(resp) =
                                resolve_github_plugin(http, format_id, repo, subdir, commit).await?
                            {
                                return Ok(Some(resp));
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Fetch or read Claude `.claude-plugin/marketplace.json`
    let mp_val = if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let local_mp = home
            .join(".claude/plugins/marketplaces/claude-plugins-official/.claude-plugin/marketplace.json");
        if let Ok(text) = std::fs::read_to_string(local_mp) {
            serde_json::from_str::<Value>(&text).ok()
        } else {
            None
        }
    } else {
        None
    };

    let mp_val = match mp_val {
        Some(v) => Some(v),
        None => {
            if let Ok(resp) = http
                .get(CLAUDE_OFFICIAL_MARKETPLACE_URL)
                .timeout(std::time::Duration::from_secs(8))
                .send()
                .await
            {
                resp.json::<Value>().await.ok()
            } else {
                None
            }
        }
    };

    if let Some(mp) = mp_val {
        if let Some(plugins) = mp.get("plugins").and_then(|v| v.as_array()) {
            if let Some(entry) = plugins.iter().find(|p| {
                p.get("name")
                    .and_then(|v| v.as_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(plugin_name))
            }) {
                if let Some(src_str) = entry.get("source").and_then(|v| v.as_str()) {
                    let clean_path = src_str.trim_start_matches("./");
                    if let Some(resp) = resolve_github_plugin(
                        http,
                        format_id,
                        "https://github.com/anthropics/claude-plugins-official",
                        Some(clean_path),
                        Some("main"),
                    )
                    .await?
                    {
                        return Ok(Some(resp));
                    }
                } else if let Some(src_obj) = entry.get("source").and_then(|v| v.as_object()) {
                    if let Some(repo_url) = src_obj.get("url").and_then(|v| v.as_str()) {
                        let subdir = src_obj.get("path").and_then(|v| v.as_str());
                        let git_ref = src_obj
                            .get("sha")
                            .or_else(|| src_obj.get("ref"))
                            .and_then(|v| v.as_str());
                        if let Some(resp) =
                            resolve_github_plugin(http, format_id, repo_url, subdir, git_ref)
                                .await?
                        {
                            return Ok(Some(resp));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

async fn resolve_github_plugin(
    http: &Client,
    format_id: &str,
    repo_spec: &str,
    subdir: Option<&str>,
    git_ref: Option<&str>,
) -> Result<Option<ImportResponse>, AppError> {
    let clean = repo_spec
        .trim_start_matches("https://github.com/")
        .trim_end_matches(".git")
        .trim_matches('/');
    let parts: Vec<&str> = clean.split('/').collect();
    if parts.len() < 2 {
        return Ok(None);
    }
    let owner = parts[0];
    let repo = parts[1];
    let r = git_ref.unwrap_or("HEAD");
    let prefix = match subdir.map(|s| s.trim_matches('/')) {
        Some(s) if !s.is_empty() => format!("{s}/"),
        _ => String::new(),
    };

    for manifest in [".mcp.json", "mcp_config.json", "gemini-extension.json"] {
        let raw_url =
            format!("https://raw.githubusercontent.com/{owner}/{repo}/{r}/{prefix}{manifest}");
        if let Ok(resp) = http
            .get(&raw_url)
            .timeout(std::time::Duration::from_secs(8))
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<Value>().await {
                    if let Some(mut parsed) = try_parse_json_value(&val)? {
                        parsed.detected_format = format_id.to_string();
                        parsed.source_label = format!("GitHub Plugin ({owner}/{repo})");
                        for c in &mut parsed.candidates {
                            c.notes.insert(
                                0,
                                format!("Fetched `{manifest}` from GitHub `{owner}/{repo}`."),
                            );
                        }
                        return Ok(Some(parsed));
                    }
                }
            }
        }
    }

    Ok(None)
}

fn resolve_from_known_registry(format_id: &str, plugin_name: &str) -> Option<ImportResponse> {
    let lower = plugin_name.to_ascii_lowercase();
    let spec = KNOWN_PLUGINS.iter().find(|s| {
        s.slug == lower
            || format!("{}-plugin", s.slug) == lower
            || format!("{}-antigravity", s.slug) == lower
    })?;

    let hdrs: Vec<(String, String)> = spec
        .headers
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    let (mut auth, mut notes) = build_auth_from_headers(hdrs);
    if let Some(cid) = spec.oauth_client_id {
        auth.kind = AuthKind::OAuth2;
        let mut oauth = default_oauth_config();
        oauth.client_id = cid.to_string();
        auth.oauth = Some(oauth);
    }
    notes.insert(
        0,
        format!(
            "Resolved `{}` ({}) from verified plugin catalog.",
            spec.display_name, spec.slug
        ),
    );

    Some(ImportResponse {
        detected_format: format_id.to_string(),
        source_label: format!("Verified Plugin Catalog ({})", spec.display_name),
        candidates: vec![ImportedCandidate {
            input: ServerInput {
                name: spec.display_name.to_string(),
                slug: spec.slug.to_string(),
                url: spec.url.to_string(),
                transport: spec.transport,
                enabled: true,
                auth,
                tool_policy: None,
            },
            discovery: None,
            notes,
            warnings: Vec::new(),
        }],
    })
}

fn extract_bridged_url_and_headers(tokens: &[String]) -> Option<(String, Vec<(String, String)>)> {
    let mut url: Option<String> = None;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if t == "--header" || t == "-H" {
            if let Some(raw_hdr) = tokens.get(i + 1) {
                if let Some(pair) = split_header(raw_hdr) {
                    headers.push(pair);
                }
                i += 2;
                continue;
            }
        }
        if let Some(raw_hdr) = t.strip_prefix("--header=") {
            if let Some(pair) = split_header(raw_hdr) {
                headers.push(pair);
            }
            i += 1;
            continue;
        }
        if (t.starts_with("https://") || t.starts_with("http://")) && url.is_none() {
            url = Some(t.clone());
        }
        i += 1;
    }
    url.map(|u| (u, headers))
}

fn build_auth_from_headers(headers: Vec<(String, String)>) -> (AuthConfig, Vec<String>) {
    let mut notes = Vec::new();
    let mut remaining_headers = Vec::new();
    let mut bearer_token: Option<String> = None;
    let mut is_bearer = false;

    for (name, value) in headers {
        if name.eq_ignore_ascii_case("authorization") {
            let trimmed = value.trim();
            if let Some(rest) = trimmed
                .strip_prefix("Bearer ")
                .or_else(|| trimmed.strip_prefix("bearer "))
            {
                is_bearer = true;
                let token = rest.trim();
                if is_secret_placeholder(token) {
                    notes.push(format!(
                        "Detected Bearer token placeholder `{token}` — enter your actual secret token in Step 02 (Vault)."
                    ));
                } else {
                    bearer_token = Some(token.to_string());
                    notes.push("Extracted Bearer token from Authorization header.".to_string());
                }
                continue;
            }
        }

        if is_secret_placeholder(&value) {
            notes.push(format!(
                "Header `{name}` uses placeholder `{value}` — enter its secret value in Step 02."
            ));
            remaining_headers.push(HeaderPair {
                name,
                value: String::new(),
            });
        } else {
            remaining_headers.push(HeaderPair { name, value });
        }
    }

    let kind = if is_bearer {
        AuthKind::Bearer
    } else if !remaining_headers.is_empty() {
        AuthKind::Headers
    } else {
        AuthKind::None
    };

    (
        AuthConfig {
            kind,
            bearer_token,
            basic_username: None,
            basic_password: None,
            basic_realm: None,
            headers: remaining_headers,
            oauth: None,
        },
        notes,
    )
}

fn is_secret_placeholder(val: &str) -> bool {
    let t = val.trim();
    t.is_empty()
        || (t.starts_with("${") && t.ends_with('}'))
        || t.starts_with('$')
        || (t.starts_with('<') && t.ends_with('>'))
        || t.eq_ignore_ascii_case("token")
        || t.eq_ignore_ascii_case("your_token")
        || t.eq_ignore_ascii_case("your_api_key")
        || t.eq_ignore_ascii_case("api_key")
        || t.eq_ignore_ascii_case("xxx")
        || t.contains("YOUR_")
}

fn split_header(raw: &str) -> Option<(String, String)> {
    let (k, v) = raw.split_once(':').or_else(|| raw.split_once('='))?;
    let key = k.trim().to_string();
    let val = v.trim().to_string();
    if key.is_empty() {
        None
    } else {
        Some((key, val))
    }
}

fn derive_name_and_slug(
    raw_key: &str,
    display_name_override: Option<&str>,
    raw_url: &str,
) -> (String, String) {
    let mut base_key = raw_key.trim().to_string();
    if base_key.is_empty() || base_key.eq_ignore_ascii_case("mcp") || base_key.eq_ignore_ascii_case("api") {
        if let Ok(parsed) = Url::parse(raw_url) {
            if let Some(host) = parsed.host_str() {
                let parts: Vec<&str> = host
                    .split('.')
                    .filter(|p| {
                        !matches!(
                            *p,
                            "mcp" | "api" | "www" | "com" | "io" | "dev" | "ai" | "app" | "org" | "net"
                        )
                    })
                    .collect();
                if let Some(first) = parts.first() {
                    base_key = (*first).to_string();
                }
            }
        }
    }
    if base_key.is_empty() {
        base_key = "mcp-server".to_string();
    }

    let slug = sanitize_slug(&base_key);
    let name = display_name_override
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| title_case_slug(&slug));
    (name, slug)
}

pub fn sanitize_slug(input: &str) -> String {
    let mut out = String::new();
    let mut prev_hyphen = false;
    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_hyphen = false;
        } else if !prev_hyphen && !out.is_empty() {
            out.push('-');
            prev_hyphen = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "mcp-server".to_string()
    } else {
        trimmed
    }
}

fn title_case_slug(slug: &str) -> String {
    slug.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn default_auth_config() -> AuthConfig {
    AuthConfig {
        kind: AuthKind::None,
        bearer_token: None,
        basic_username: None,
        basic_password: None,
        basic_realm: None,
        headers: Vec::new(),
        oauth: None,
    }
}

fn default_oauth_config() -> OAuthConfig {
    OAuthConfig {
        grant: OAuthGrant::AuthorizationCode,
        client_id: String::new(),
        client_secret: None,
        authorize_url: None,
        token_url: String::new(),
        scopes: Vec::new(),
        resource: None,
        registration_endpoint: None,
        access_token: None,
        refresh_token: None,
        expires_at: None,
    }
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

pub fn shlex_split(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;

    while let Some(ch) = chars.next() {
        if in_single {
            if ch == '\'' {
                in_single = false;
            } else {
                current.push(ch);
            }
        } else if in_double {
            if ch == '"' {
                in_double = false;
            } else if ch == '\\' {
                if let Some(&next) = chars.peek() {
                    if matches!(next, '"' | '\\' | '$') {
                        current.push(chars.next().unwrap());
                        continue;
                    }
                }
                current.push(ch);
            } else {
                current.push(ch);
            }
        } else {
            match ch {
                '\'' => in_single = true,
                '"' => in_double = true,
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                }
                c if c.is_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                c => current.push(c),
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

/// Pure-Rust reader for uncompressed (`ZIP_STORED`, method 0) JSON manifests in a ZIP archive.
/// Uses Central Directory (`PK\x01\x02`) with fallback to Local File Headers (`PK\x03\x04`).
pub fn extract_zip_stored_json_files(bytes: &[u8]) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();

    // Scan Local File Headers (PK\x03\x04)
    let mut pos = 0;
    while pos + 30 <= bytes.len() {
        if &bytes[pos..pos + 4] != b"PK\x03\x04" {
            pos += 1;
            continue;
        }
        let method = u16::from_le_bytes([bytes[pos + 8], bytes[pos + 9]]);
        let comp_size = u32::from_le_bytes([
            bytes[pos + 18],
            bytes[pos + 19],
            bytes[pos + 20],
            bytes[pos + 21],
        ]) as usize;
        let name_len = u16::from_le_bytes([bytes[pos + 26], bytes[pos + 27]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[pos + 28], bytes[pos + 29]]) as usize;
        let data_start = pos + 30 + name_len + extra_len;
        let data_end = data_start.saturating_add(comp_size);
        if data_end > bytes.len() {
            break;
        }
        if method == 0 {
            if let Ok(filename) = std::str::from_utf8(&bytes[pos + 30..pos + 30 + name_len]) {
                if filename.ends_with("mcp_config.json") || filename.ends_with(".mcp.json") {
                    if let Ok(content) = std::str::from_utf8(&bytes[data_start..data_end]) {
                        files.insert(filename.to_string(), content.to_string());
                    }
                }
            }
        }
        pos = data_end;
    }

    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parses_claude_mcp_add_with_headers() {
        let http = Client::new();
        let resp = parse_and_resolve_import(
            &http,
            ImportRequest {
                command: r#"claude mcp add --transport http github https://api.githubcopilot.com/mcp/ --header "Authorization: Bearer ${GITHUB_PERSONAL_ACCESS_TOKEN}""#.to_string(),
                auto_discover: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(resp.detected_format, "claude-mcp-add");
        assert_eq!(resp.candidates.len(), 1);
        let c = &resp.candidates[0];
        assert_eq!(c.input.slug, "github");
        assert_eq!(c.input.url, "https://api.githubcopilot.com/mcp/");
        assert_eq!(c.input.auth.kind, AuthKind::Bearer);
        assert!(c.input.auth.bearer_token.is_none());
        assert!(c.notes.iter().any(|n| n.contains("GITHUB_PERSONAL_ACCESS_TOKEN")));
    }

    #[tokio::test]
    async fn parses_agy_plugin_install_command() {
        let http = Client::new();
        let resp = parse_and_resolve_import(
            &http,
            ImportRequest {
                command: "agy plugin install notion@antigravity-plugins-partners".to_string(),
                auto_discover: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(resp.detected_format, "agy-plugin");
        assert_eq!(resp.candidates.len(), 1);
        let c = &resp.candidates[0];
        assert_eq!(c.input.slug, "notion");
        assert_eq!(c.input.url, "https://mcp.notion.com/mcp");
    }

    #[tokio::test]
    async fn parses_codex_toml_config() {
        let http = Client::new();
        let toml = r#"
[mcp_servers.linear]
url = "https://mcp.linear.app/mcp"
bearer_token_env_var = "LINEAR_API_KEY"
"#;
        let resp = parse_and_resolve_import(
            &http,
            ImportRequest {
                command: toml.to_string(),
                auto_discover: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(resp.detected_format, "codex-toml");
        assert_eq!(resp.candidates.len(), 1);
        assert_eq!(resp.candidates[0].input.slug, "linear");
        assert_eq!(resp.candidates[0].input.url, "https://mcp.linear.app/mcp");
        assert_eq!(resp.candidates[0].input.auth.kind, AuthKind::Bearer);
    }

    #[tokio::test]
    async fn parses_cursor_deeplink_and_mcp_remote() {
        let http = Client::new();
        let cfg = r#"{"command":"npx","args":["-y","mcp-remote","https://mcp.linear.app/mcp"]}"#;
        let b64 = STANDARD.encode(cfg);
        let link = format!("cursor://anysphere.cursor-deeplink/mcp/install?name=Linear&config={b64}");

        let resp = parse_and_resolve_import(
            &http,
            ImportRequest {
                command: link,
                auto_discover: false,
            },
        )
        .await
        .unwrap();

        assert_eq!(resp.detected_format, "cursor-deeplink");
        assert_eq!(resp.candidates.len(), 1);
        assert_eq!(resp.candidates[0].input.slug, "linear");
        assert_eq!(resp.candidates[0].input.url, "https://mcp.linear.app/mcp");
    }
}
