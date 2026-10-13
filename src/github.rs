//! Fixed authenticated GitHub read adapter. The NdsClient credential owner
//! lends a token only for this future; it never enters a DTO or a core request.
use nddev_device_sync_domain::{
    modules::{ModuleError, query_outcome},
    repositories::catalog::*,
};
use serde::Deserialize;
use std::time::Duration;
pub use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use zeroize::Zeroizing;

#[cfg(not(target_arch = "wasm32"))]
#[path = "github/native.rs"]
mod platform;
#[cfg(target_arch = "wasm32")]
#[path = "github/browser.rs"]
mod platform;

const ORIGIN: &str = "https://api.github.com";
const API_VERSION: &str = "2026-03-10";
const MAX_BODY: usize = 1_048_576;
const DEADLINE: Duration = Duration::from_secs(10);

/// `revalidate` is mandatory: the NdsClient owner rechecks its actual Accounts
/// fence and selected Vault version before each GET and before result return.
/// All three checks are inside the one deadline/cancellation lifetime.
pub async fn read_user_and_page(
    credential: &str,
    request: &CatalogPageRequest,
    cancellation: &CancellationToken,
    mut revalidate: impl AsyncFnMut() -> Result<(), ModuleError>,
) -> Result<ProviderRepositoryPage, ModuleError> {
    let span = nddev_device_sync_telemetry::operation_span("gds", "io")
        .map_err(|_| ModuleError::InvalidContract)?;
    async {
        let result = bounded_read(credential, request, cancellation, &mut revalidate).await;
        nddev_device_sync_telemetry::finish_operation_span(
            &tracing::Span::current(), query_outcome(&result).terminal_failure(),
        );
        match &result {
            Ok(_) => tracing::info!(event.name="gds.catalog.read.completed", module="gds", scope="io", outcome="ok"),
            Err(error) => tracing::warn!(event.name="gds.catalog.read.failed", module="gds", scope="io", error.type=%error, outcome="error"),
        }
        result
    }.instrument(span).await
}

async fn bounded_read(
    credential: &str,
    request: &CatalogPageRequest,
    cancellation: &CancellationToken,
    revalidate: &mut impl AsyncFnMut() -> Result<(), ModuleError>,
) -> Result<ProviderRepositoryPage, ModuleError> {
    if credential.is_empty()
        || credential.len() > 16_384
        || !credential.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(ModuleError::InvalidInput);
    }
    if cancellation.is_cancelled() {
        return Err(ModuleError::Cancelled);
    }
    let client = platform::Client::new()?;
    bounded_client(&client, credential, request, cancellation, revalidate).await
}
async fn bounded_client(
    client: &platform::Client,
    credential: &str,
    request: &CatalogPageRequest,
    cancellation: &CancellationToken,
    revalidate: &mut impl AsyncFnMut() -> Result<(), ModuleError>,
) -> Result<ProviderRepositoryPage, ModuleError> {
    let work = read(client, credential, request, revalidate);
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::select! { biased;
            () = cancellation.cancelled() => Err(ModuleError::Cancelled),
            result = tokio::time::timeout(DEADLINE, work) => result.unwrap_or(Err(ModuleError::Timeout)),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        tokio::select! { biased;
            () = cancellation.cancelled() => Err(ModuleError::Cancelled),
            () = gloo_timers::future::TimeoutFuture::new(DEADLINE.as_millis() as u32) => Err(ModuleError::Timeout),
            result = work => result,
        }
    }
}

async fn read(
    client: &platform::Client,
    credential: &str,
    request: &CatalogPageRequest,
    revalidate: &mut impl AsyncFnMut() -> Result<(), ModuleError>,
) -> Result<ProviderRepositoryPage, ModuleError> {
    revalidate().await?;
    let principal = {
        let response = client.get(credential, Route::User).await?;
        let value: OwnerWire =
            serde_json::from_slice(&response.body).map_err(|_| ModuleError::InvalidResponse)?;
        value.decode()?
    };
    if principal.kind != GithubOwnerKind::User
        || request
            .expected_principal()
            .is_some_and(|id| id != principal.id)
    {
        return Err(ModuleError::InvalidResponse);
    }
    revalidate().await?;
    let response = client
        .get(credential, Route::Repositories(request.page()))
        .await?;
    let has_more = next_page(response.link.as_deref(), request.page())?;
    let rows: Vec<RepositoryWire> =
        serde_json::from_slice(&response.body).map_err(|_| ModuleError::InvalidResponse)?;
    if rows.len() > REPOSITORIES_PER_PAGE {
        return Err(ModuleError::InvalidResponse);
    }
    let repositories = rows
        .into_iter()
        .map(RepositoryWire::decode)
        .collect::<Result<_, _>>()?;
    revalidate().await?;
    Ok(ProviderRepositoryPage {
        principal,
        repositories,
        observed_at_ms: now_ms()?,
        has_more,
    })
}

enum Route {
    User,
    Repositories(u8),
}
impl Route {
    fn path(&self) -> String {
        match self {
            Self::User => "/user".into(),
            Self::Repositories(page) => format!(
                "/user/repos?affiliation=owner,organization_member&visibility=all&sort=full_name&per_page={REPOSITORIES_PER_PAGE}&page={page}"
            ),
        }
    }
}
struct Reply {
    body: Zeroizing<Vec<u8>>,
    link: Option<String>,
}
fn status(
    status: u16,
    rate_remaining: Option<&str>,
    retry_after: Option<&str>,
) -> Result<(), ModuleError> {
    tracing::info!(
        event.name = "gds.http.response",
        module = "gds",
        scope = "io",
        http.response.status_code = status,
        outcome = if status == 200 { "ok" } else { "rejected" }
    );
    match status {
        200 => Ok(()),
        429 => Err(ModuleError::Busy),
        403 if rate_remaining == Some("0") || retry_after.is_some() => Err(ModuleError::Busy),
        401 | 403 | 404 => Err(ModuleError::Unavailable),
        300..=399 => Err(ModuleError::InvalidResponse),
        _ => Err(ModuleError::ProviderFailed),
    }
}
fn next_page(link: Option<&str>, current: u8) -> Result<bool, ModuleError> {
    let Some(link) = link else { return Ok(false) };
    if link.len() > 8192 {
        return Err(ModuleError::InvalidResponse);
    }
    let mut found = false;
    for part in link.split(", ") {
        let Some((target, relation)) = part.split_once(';') else {
            return Err(ModuleError::InvalidResponse);
        };
        if !relation
            .split(';')
            .any(|value| value.trim() == "rel=\"next\"")
        {
            continue;
        }
        if found {
            return Err(ModuleError::InvalidResponse);
        }
        let target = target
            .trim()
            .strip_prefix('<')
            .and_then(|v| v.strip_suffix('>'))
            .ok_or(ModuleError::InvalidResponse)?;
        let url = url::Url::parse(target).map_err(|_| ModuleError::InvalidResponse)?;
        if url.origin().ascii_serialization() != ORIGIN
            || url.path() != "/user/repos"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(ModuleError::InvalidResponse);
        }
        let mut pairs = std::collections::BTreeMap::new();
        for (key, value) in url.query_pairs() {
            if pairs.insert(key, value).is_some() {
                return Err(ModuleError::InvalidResponse);
            }
        }
        if pairs.get("page").and_then(|n| n.parse::<u8>().ok()) != current.checked_add(1)
            || pairs.get("per_page").map(|v| v.as_ref()) != Some("25")
            || pairs.get("affiliation").map(|v| v.as_ref()) != Some("owner,organization_member")
            || pairs.get("visibility").map(|v| v.as_ref()) != Some("all")
            || pairs.get("sort").map(|v| v.as_ref()) != Some("full_name")
            || pairs.keys().any(|k| {
                !matches!(
                    k.as_ref(),
                    "page" | "per_page" | "affiliation" | "visibility" | "sort" | "direction"
                )
            })
        {
            return Err(ModuleError::InvalidResponse);
        }
        found = true;
    }
    Ok(found)
}

#[derive(Deserialize)]
struct OwnerWire {
    id: u64,
    login: String,
    #[serde(rename = "type")]
    kind: String,
}
impl OwnerWire {
    fn decode(self) -> Result<GithubOwner, ModuleError> {
        let kind = match self.kind.as_str() {
            "User" => GithubOwnerKind::User,
            "Organization" => GithubOwnerKind::Organization,
            _ => return Err(ModuleError::InvalidResponse),
        };
        Ok(GithubOwner {
            id: GithubAccountId::new(self.id)?,
            kind,
            login: self.login,
        })
    }
}
#[derive(Deserialize)]
struct RepositoryWire {
    id: u64,
    owner: OwnerWire,
    name: String,
    full_name: String,
    visibility: String,
    archived: bool,
    default_branch: Option<String>,
}
impl RepositoryWire {
    fn decode(self) -> Result<RepositorySummary, ModuleError> {
        let owner = self.owner.decode()?;
        if self.full_name != format!("{}/{}", owner.login, self.name) {
            return Err(ModuleError::InvalidResponse);
        }
        let visibility = match self.visibility.as_str() {
            "public" => RepositoryVisibility::Public,
            "private" => RepositoryVisibility::Private,
            "internal" => RepositoryVisibility::Internal,
            _ => return Err(ModuleError::InvalidResponse),
        };
        Ok(RepositorySummary {
            id: RepositoryId::new(self.id)?,
            owner,
            name: self.name,
            visibility,
            archived: self.archived,
            default_branch: self.default_branch,
        })
    }
}
fn now_ms() -> Result<u64, ModuleError> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|v| v.as_millis().try_into().ok())
            .filter(|v| *v > 0)
            .ok_or(ModuleError::InvalidResponse)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let value = js_sys::Date::now();
        if value.is_finite()
            && value > 0.0
            && value <= nddev_device_sync_domain::sync::MAX_REVISION as f64
        {
            Ok(value as u64)
        } else {
            Err(ModuleError::InvalidResponse)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_decode_and_link_checks_are_not_provider_acceptance() {
        let json = br#"{"id":8,"owner":{"id":3,"login":"org","type":"Organization"},"name":"repo","full_name":"org/repo","visibility":"private","archived":false,"default_branch":"main"}"#;
        let row: RepositoryWire = serde_json::from_slice(json).unwrap();
        assert_eq!(row.decode().unwrap().id.value(), 8);
        let base = "https://api.github.com/user/repos?affiliation=owner%2Corganization_member&visibility=all&sort=full_name&per_page=25&page=2";
        assert_eq!(
            next_page(Some(&format!("<{base}>; rel=\"next\"")), 1),
            Ok(true)
        );
        assert!(next_page(Some("<https://other.invalid/user/repos>; rel=\"next\""), 1).is_err());
        assert!(next_page(Some(&format!("<{base}>; rel=\"next\"")), 2).is_err());
        assert_eq!(status(401, None, None), Err(ModuleError::Unavailable));
        assert_eq!(status(403, Some("0"), None), Err(ModuleError::Busy));
        assert_eq!(status(302, None, None), Err(ModuleError::InvalidResponse));
    }
}
