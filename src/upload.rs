use anyhow::Result;

#[allow(dead_code)]
pub struct UploadPolicy {
    pub upload_url: String,
    pub asset_id: u64,
    pub asset_href: String,
    pub asset_upload_authenticity_token: String,
    pub form_fields: std::collections::HashMap<String, String>,
}

pub fn parse_upload_token(html: &str) -> Result<String> {
    let marker = r#""uploadToken":""#;
    if let Some(start) = html.find(marker) {
        let rest = &html[start + marker.len()..];
        if let Some(end) = rest.find('"') {
            return Ok(rest[..end].to_string());
        }
    }
    anyhow::bail!("uploadToken not found in page HTML")
}

pub fn parse_upload_policy_response(json: &str) -> Result<UploadPolicy> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let upload_url = v["upload_url"].as_str().ok_or_else(|| anyhow::anyhow!("missing upload_url"))?.to_string();
    let asset_id = v["asset"]["id"].as_u64().ok_or_else(|| anyhow::anyhow!("missing asset.id"))?;
    let asset_href = v["asset"]["href"].as_str().ok_or_else(|| anyhow::anyhow!("missing asset.href"))?.to_string();
    let asset_upload_authenticity_token = v["asset_upload_authenticity_token"].as_str().ok_or_else(|| anyhow::anyhow!("missing asset_upload_authenticity_token"))?.to_string();
    let form_fields = v["form"].as_object()
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
        .unwrap_or_default();
    Ok(UploadPolicy { upload_url, asset_id, asset_href, asset_upload_authenticity_token, form_fields })
}

pub fn inject_demo_section(body: &str, urls: &[String]) -> String {
    let demo_section = {
        let images: String = urls.iter().enumerate()
            .map(|(i, url)| format!("![Demo {}]({})", i + 1, url))
            .collect::<Vec<_>>()
            .join("\n");
        format!("## Demo\n\n{}", images)
    };
    if let Some(pos) = body.find("\n## Demo") {
        format!("{}\n\n{}", body[..pos].trim_end(), demo_section)
    } else if body.contains("## Demo") && body.starts_with("## Demo") {
        demo_section
    } else {
        format!("{}\n\n{}", body.trim_end(), demo_section)
    }
}

pub fn mime_type_from_filename(filename: &str) -> &'static str {
    match filename.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

#[allow(dead_code)]
pub fn parse_gh_image_output(output: &str) -> Result<String> {
    for line in output.lines() {
        if let Some(start) = line.find("](")
            && let Some(end) = line[start + 2..].find(')')
        {
            return Ok(line[start + 2..start + 2 + end].to_string());
        }
    }
    anyhow::bail!("could not parse URL from gh image output: {:?}", output)
}

pub fn github_upload_image(
    file_path: &std::path::Path,
    owner: &str,
    repo: &str,
    api_client: &crate::github::GithubClient,
    session_cookie: &str,
) -> Result<String> {
    let file_bytes = std::fs::read(file_path)?;
    let filename = file_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("invalid filename"))?;
    let content_type = mime_type_from_filename(filename);

    let repo_info = api_client.get(&format!("/repos/{}/{}", owner, repo))?;
    let repo_id = repo_info["id"].as_u64().ok_or_else(|| anyhow::anyhow!("missing repo id"))?;

    let cookie_header = format!(
        "user_session={}; __Host-user_session_same_site={}",
        session_cookie, session_cookie
    );

    let http = reqwest::blocking::Client::builder().user_agent("fp/0.1").build()?;

    let page_html = http.get(format!("https://github.com/{}/{}", owner, repo))
        .header("Cookie", &cookie_header)
        .send()?.text()?;
    let upload_token = parse_upload_token(&page_html)?;

    let policy_resp = http.post("https://github.com/upload/policies/assets")
        .header("Cookie", &cookie_header)
        .header("Accept", "application/json")
        .header("Origin", "https://github.com")
        .header("Referer", format!("https://github.com/{}/{}", owner, repo))
        .header("X-Requested-With", "XMLHttpRequest")
        .multipart(
            reqwest::blocking::multipart::Form::new()
                .text("name", filename.to_string())
                .text("size", file_bytes.len().to_string())
                .text("content_type", content_type.to_string())
                .text("authenticity_token", upload_token)
                .text("repository_id", repo_id.to_string()),
        )
        .send()?.text()?;
    let policy = parse_upload_policy_response(&policy_resp)?;

    let mut form = reqwest::blocking::multipart::Form::new();
    for (k, v) in &policy.form_fields {
        form = form.text(k.clone(), v.clone());
    }
    form = form.part("file", reqwest::blocking::multipart::Part::bytes(file_bytes)
        .file_name(filename.to_string())
        .mime_str(content_type)?);
    let s3_status = http.post(&policy.upload_url).multipart(form).send()?.status();
    if !s3_status.is_success() && s3_status.as_u16() != 204 {
        anyhow::bail!("S3 upload failed with status {}", s3_status);
    }

    let finalize_resp: serde_json::Value = http
        .put(format!("https://github.com/upload/assets/{}", policy.asset_id))
        .header("Cookie", &cookie_header)
        .header("Accept", "application/json")
        .multipart(
            reqwest::blocking::multipart::Form::new()
                .text("authenticity_token", policy.asset_upload_authenticity_token),
        )
        .send()?.json()?;
    let href = finalize_resp["href"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing href in finalize response"))?
        .to_string();
    Ok(href)
}
