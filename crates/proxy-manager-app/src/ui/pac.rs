use super::*;
use proxy_config::geodata::{GeoData, GeoSiteCategorySummary};
use proxy_model::{PacMode, hash};
use proxy_pac::PacDomainKind;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

const QUICK_GEOSITES: &[(&str, &str)] = &[
    ("private", "局域网"),
    ("cn", "国内常见"),
    ("geolocation-!cn", "海外常见"),
    ("category-ads-all", "广告屏蔽"),
    ("apple-cn", "苹果国内"),
    ("google", "Google"),
    ("github", "GitHub"),
];

fn hash_list_and_context(
    geosites: &[String],
    domains: &[String],
    config_json: Option<&str>,
    geo_stamp: Option<u64>,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    geosites.hash(&mut hasher);
    domains.hash(&mut hasher);
    config_json.hash(&mut hasher);
    geo_stamp.hash(&mut hasher);
    hasher.finish()
}

fn kind_label(kind: PacDomainKind) -> &'static str {
    match kind {
        PacDomainKind::Exact => "精确",
        PacDomainKind::Subdomain => "仅子域",
        PacDomainKind::DomainAndSub => "含主域",
    }
}

fn mode_label(mode: PacMode) -> &'static str {
    match mode {
        PacMode::AutoRules => "按当前路由规则自动生成（推荐）",
        PacMode::DirectList => "选中 GeoSite 与域名直连，其余代理",
        PacMode::ProxyList => "仅代理选中 GeoSite 与域名，其余直连",
        PacMode::All => "全部代理（本地/局域网直连）",
        PacMode::Custom => "自定义 PAC 脚本",
    }
}

fn format_invalid_preview(invalid: &[(usize, String, String)]) -> String {
    let preview: Vec<String> = invalid
        .iter()
        .take(5)
        .map(|(line, token, _)| format!("第 {line} 行：{token}"))
        .collect();
    if invalid.len() > 5 {
        format!("{} 等共 {} 项无效", preview.join("；"), invalid.len())
    } else {
        preview.join("；")
    }
}

impl App {
    #[cfg(test)]
    pub(super) fn refresh_geodata_cache(&mut self) -> Option<GeoDataCache> {
        let editor = self.editor.clone();
        self.refresh_geodata_cache_for(&editor)
    }

    pub(in crate::app) fn refresh_geodata_cache_for(
        &mut self,
        config_json: &str,
    ) -> Option<GeoDataCache> {
        let asset_root = self.asset_root.trim().to_owned();
        if asset_root.is_empty() {
            self.ui.geodata_cache = None;
            return None;
        }
        let resolved_root = resolve_path(&self.root, std::path::Path::new(&asset_root));
        if !resolved_root.is_dir() {
            self.ui.geodata_cache = None;
            return None;
        }
        let parsed_config = serde_json::from_str::<serde_json::Value>(config_json).ok();
        let mut tracked_files =
            std::collections::BTreeSet::from(["geosite.dat".to_owned(), "geoip.dat".to_owned()]);
        if let Some(value) = &parsed_config {
            for name in proxy_config::resources::collect_referenced_geo_files(value) {
                if name.to_ascii_lowercase().ends_with(".dat") {
                    tracked_files.insert(name);
                }
            }
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for name in &tracked_files {
            name.hash(&mut hasher);
            if let Ok(meta) = std::fs::metadata(resolved_root.join(name)) {
                true.hash(&mut hasher);
                meta.len().hash(&mut hasher);
                meta.modified().ok().hash(&mut hasher);
            } else {
                false.hash(&mut hasher);
            }
        }
        let stamp = hasher.finish();
        if let Some(cached) = &self.ui.geodata_cache
            && cached.asset_root == asset_root
            && cached.stamp == stamp
        {
            return Some(cached.clone());
        }
        let mut data = GeoData::load_from_asset_root_best_effort(&resolved_root);
        let geosite_error = (resolved_root.join("geosite.dat").is_file() && data.geosite.is_none())
            .then(|| {
                "geosite.dat 解析失败或为空文件（请确认是标准 Protobuf .dat 文件，切勿将 .db/.srs 改名）。"
                    .to_owned()
            });
        if let Some(value) = &parsed_config {
            data.preload_ext_from_config(value);
        }
        let categories = data.categories();
        let entry = GeoDataCache {
            asset_root,
            stamp,
            data,
            categories,
            geosite_error,
        };
        self.ui.geodata_cache = Some(entry.clone());
        Some(entry)
    }

    fn update_pac_check_state(&mut self, script: &str, mut notes: Vec<String>) {
        match proxy_pac::validate(script) {
            Ok(()) => {
                self.ui.pac_checked = true;
                for note in proxy_pac::compat_notes(script) {
                    if !notes.contains(&note) {
                        notes.push(note);
                    }
                }
                self.ui.pac_notes = notes;
            }
            Err(error) => {
                self.ui.pac_checked = false;
                notes.insert(0, format!("语法校验未通过：{}", error.message));
                self.ui.pac_notes = notes;
            }
        }
    }

    fn sync_pac_script_for(
        &mut self,
        profile_id: &str,
        config_json: &str,
        draft: &mut PacDraft,
        endpoint: Option<std::net::SocketAddr>,
        cached_geo: Option<&GeoDataCache>,
    ) -> bool {
        if draft.mode == PacMode::Custom {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            draft.script.hash(&mut hasher);
            let script_hash = hasher.finish();
            let custom_key = (
                profile_id.to_owned(),
                std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
                PacMode::Custom,
                script_hash,
            );
            if self.ui.pac_compile_key.as_ref() != Some(&custom_key) {
                self.ui.pac_compile_key = Some(custom_key);
                self.ui.pac_test.clear();
                self.ui.pac_compile_stats = proxy_pac::PacCompileStats {
                    script_bytes: draft.script.len(),
                    ..Default::default()
                };
                if draft.script.trim().is_empty() {
                    self.ui.pac_checked = false;
                    self.ui.pac_notes.clear();
                } else {
                    self.update_pac_check_state(&draft.script, Vec::new());
                }
            }
            return false;
        }

        let Some(endpoint) = endpoint else {
            return false;
        };
        let config_str = matches!(draft.mode, PacMode::AutoRules).then_some(config_json);
        let key = (
            profile_id.to_owned(),
            endpoint,
            draft.mode,
            hash_list_and_context(
                &draft.geosites,
                &draft.domains,
                config_str,
                cached_geo.map(|c| c.stamp),
            ),
        );
        if self.ui.pac_compile_key.as_ref() == Some(&key) && !draft.script.is_empty() {
            return false;
        }
        self.ui.pac_compile_key = Some(key);
        let config_value = serde_json::from_str::<serde_json::Value>(config_json)
            .unwrap_or_else(|_| serde_json::json!({}));
        match proxy_pac::compile_draft(&config_value, endpoint, draft, cached_geo.map(|c| &c.data))
        {
            Ok(generated) => {
                let script_changed = draft.script != generated.script;
                if script_changed {
                    draft.script = generated.script;
                    self.ui.pac_test.clear();
                }
                self.ui.pac_compile_stats = generated.stats;
                self.update_pac_check_state(&draft.script, generated.notes);
                script_changed
            }
            Err(error) => {
                self.ui.pac_checked = false;
                self.ui.pac_notes = vec![error.message.clone()];
                self.message = error.message;
                false
            }
        }
    }

    fn is_pac_sync_pending(&self) -> bool {
        self.busy && self.pending.values().any(|p| p.poll.is_none())
    }

    fn auto_sync_running_pac_if_active(&mut self, profile_id: &str, draft: &PacDraft) {
        let should_sync = self.is_running()
            && self.ui.pac_checked
            && self.ui.connected
            && !self.busy
            && self.ui.uncertain.is_none()
            && self.write_blocker().is_none()
            && self
                .status
                .as_ref()
                .and_then(|s| s.generation.as_ref())
                .is_some_and(|g| {
                    g.proxy_mode == ProxyMode::Pac
                        && g.candidate.profile_id == profile_id
                        && g.pac.as_ref().is_none_or(|p| {
                            p.revision != hash(draft.script.as_bytes()) || p.port != draft.port
                        })
                });
        if should_sync {
            let http_tag = self.runtime_http_tag();
            self.send(Command::SetProxyMode {
                mode: ProxyMode::Pac,
                http_inbound_tag: http_tag,
                pac: Some(draft.clone()),
            });
        }
    }

    fn commit_pac_list_change_for(
        &mut self,
        profile_id: &str,
        config_json: &str,
        draft: &mut PacDraft,
        endpoint: Option<std::net::SocketAddr>,
    ) {
        if !matches!(draft.mode, PacMode::DirectList | PacMode::ProxyList) {
            draft.mode = PacMode::ProxyList;
        }
        self.ui.pac_compile_key = None;
        let cached_geo = self.refresh_geodata_cache_for(config_json);
        self.sync_pac_script_for(
            profile_id,
            config_json,
            draft,
            endpoint,
            cached_geo.as_ref(),
        );
        self.save_pac_draft_for(profile_id, draft.clone());
        self.auto_sync_running_pac_if_active(profile_id, draft);
    }

    fn add_pac_domain_for(
        &mut self,
        profile_id: &str,
        config_json: &str,
        draft: &mut PacDraft,
        endpoint: Option<std::net::SocketAddr>,
    ) -> bool {
        let input = self.ui.pac_domain_input.trim();
        if input.is_empty() {
            return false;
        }
        let single_token = !input
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, ',' | ';'));
        if single_token {
            if let Some(spec) = proxy_pac::normalize_geosite_spec(input) {
                if draft.geosites.iter().any(|g| g.eq_ignore_ascii_case(&spec)) {
                    self.message = format!("GeoSite 分组已在清单中：{spec}");
                    return false;
                }
                draft.geosites.push(spec.clone());
                self.ui.pac_domain_input.clear();
                self.commit_pac_list_change_for(profile_id, config_json, draft, endpoint);
                self.message = format!("已添加并保存 GeoSite 分组：{spec}");
                return true;
            }
            match proxy_pac::normalize_domain(input) {
                Ok(domain) => {
                    if draft
                        .domains
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(&domain))
                    {
                        self.message = format!("域名已在清单中：{domain}");
                        return false;
                    }
                    draft.domains.push(domain.clone());
                    self.ui.pac_domain_input.clear();
                    self.commit_pac_list_change_for(profile_id, config_json, draft, endpoint);
                    self.message = format!("已添加并保存域名：{domain}");
                    true
                }
                Err(error) => {
                    self.message = error.message;
                    false
                }
            }
        } else {
            let parsed = proxy_pac::parse_domain_batch(input, &draft.domains);
            if parsed.added.is_empty() {
                self.message = if !parsed.invalid.is_empty() {
                    format!("未添加域名：{}", format_invalid_preview(&parsed.invalid))
                } else {
                    format!("输入的 {} 个域名均已在清单中。", parsed.duplicates)
                };
                return false;
            }
            let added_count = parsed.added.len();
            draft.domains.extend(parsed.added);
            self.ui.pac_domain_input.clear();
            self.commit_pac_list_change_for(profile_id, config_json, draft, endpoint);
            self.message = if parsed.invalid.is_empty() && parsed.duplicates == 0 {
                format!("已批量添加并保存 {added_count} 条域名。")
            } else {
                format!(
                    "已添加 {added_count} 条域名，跳过重复 {} 条，无效 {} 条。",
                    parsed.duplicates,
                    parsed.invalid.len()
                )
            };
            true
        }
    }

    pub(super) fn apply_pac_batch(&mut self, input: &str, replace: bool) -> bool {
        let (profile_id, config_json) = self.active_pac_target();
        let mut draft = self.pac_draft_for(&profile_id);
        let endpoint = self
            .pac_endpoint_in(&config_json, Some(&self.http_tag))
            .ok()
            .map(|(_, ep)| ep);
        let existing = if replace {
            Vec::new()
        } else {
            draft.domains.clone()
        };
        let parsed = proxy_pac::parse_domain_batch(input, &existing);
        if replace && parsed.added.is_empty() && !parsed.invalid.is_empty() {
            let detail = format!(
                "未替换清单，请修正无效条目：{}",
                format_invalid_preview(&parsed.invalid)
            );
            self.ui.pac_batch_report = Some(detail.clone());
            self.message = detail;
            return false;
        }
        if replace {
            draft.domains = parsed.added.clone();
            self.ui.pac_selected_domains.clear();
            self.commit_pac_list_change_for(&profile_id, &config_json, &mut draft, endpoint);
        } else if !parsed.added.is_empty() {
            draft.domains.extend(parsed.added.iter().cloned());
            self.commit_pac_list_change_for(&profile_id, &config_json, &mut draft, endpoint);
        }

        let summary = if replace {
            format!(
                "已替换为 {} 条域名（跳过重复 {} 条）",
                draft.domains.len(),
                parsed.duplicates
            )
        } else {
            format!(
                "已追加 {} 条域名（跳过重复 {} 条）",
                parsed.added.len(),
                parsed.duplicates
            )
        };
        if parsed.invalid.is_empty() {
            self.ui.pac_batch_report = None;
            self.message = format!("{summary}。");
            true
        } else {
            let detail = format!(
                "{summary}；以下格式无效未导入：{}",
                format_invalid_preview(&parsed.invalid)
            );
            self.ui.pac_batch_report = Some(detail.clone());
            self.message = detail;
            false
        }
    }

    pub(super) fn clear_all_pac_domains_for(&mut self, profile_id: &str) {
        let (active_id, active_json) = self.active_pac_target();
        let config_json = if active_id == profile_id {
            active_json
        } else {
            self.editor.clone()
        };
        let mut draft = self.pac_draft_for(profile_id);
        let count = draft.domains.len() + draft.geosites.len();
        draft.domains.clear();
        draft.geosites.clear();
        self.ui.pac_selected_domains.clear();
        self.ui.pac_domain_filter.clear();
        let endpoint = self
            .pac_endpoint_in(&config_json, Some(&self.http_tag))
            .ok()
            .map(|(_, ep)| ep);
        self.commit_pac_list_change_for(profile_id, &config_json, &mut draft, endpoint);
        self.message = format!("已清空 {count} 项清单规则。");
    }

    fn extract_pac_domains_from_config(
        &mut self,
        profile_id: &str,
        config_json: &str,
        draft: &mut PacDraft,
        endpoint: Option<std::net::SocketAddr>,
    ) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(config_json) else {
            self.message = "当前 JSON 无法解析，无法提取路由规则。".into();
            return;
        };
        let extracted = proxy_pac::extract_domains_from_config(&value);
        let mut existing_domains: HashSet<String> = draft
            .domains
            .iter()
            .map(|d| d.to_ascii_lowercase())
            .collect();
        let mut existing_geosites: HashSet<String> = draft
            .geosites
            .iter()
            .map(|g| g.to_ascii_lowercase())
            .collect();
        let mut added_domains = 0usize;
        let mut added_geosites = 0usize;
        let mut duplicates = 0usize;

        for tag in extracted.geosites {
            if existing_geosites.insert(tag.to_ascii_lowercase()) {
                draft.geosites.push(tag);
                added_geosites += 1;
            } else {
                duplicates += 1;
            }
        }
        for domain in extracted.domains {
            if existing_domains.insert(domain.to_ascii_lowercase()) {
                draft.domains.push(domain);
                added_domains += 1;
            } else {
                duplicates += 1;
            }
        }
        if added_domains > 0 || added_geosites > 0 {
            self.commit_pac_list_change_for(profile_id, config_json, draft, endpoint);
        }
        let mut parts = vec![format!(
            "已从路由提取：{added_geosites} 个 GeoSite 分组、{added_domains} 条域名"
        )];
        if duplicates > 0 {
            parts.push(format!("已存在 {duplicates} 项"));
        }
        let skipped = extracted.skipped_regexp + extracted.skipped_other;
        if skipped > 0 {
            parts.push(format!("跳过 {skipped} 条正则或关键字规则"));
        }
        self.message = format!("{}。", parts.join("；"));
    }

    fn dedup_and_sort_pac_domains(
        &mut self,
        profile_id: &str,
        config_json: &str,
        draft: &mut PacDraft,
        endpoint: Option<std::net::SocketAddr>,
    ) {
        let before = draft.domains.len();
        let mut seen = HashSet::new();
        let mut cleaned = Vec::with_capacity(before);
        for domain in &draft.domains {
            let normalized = proxy_pac::normalize_domain(domain).unwrap_or_else(|_| domain.clone());
            if seen.insert(normalized.to_ascii_lowercase()) {
                cleaned.push(normalized);
            }
        }
        cleaned.sort_by_key(|d| {
            d.strip_prefix("full:")
                .or_else(|| d.strip_prefix("*."))
                .unwrap_or(d)
                .to_ascii_lowercase()
        });
        let removed_dups = before.saturating_sub(cleaned.len());
        draft.domains = cleaned;
        draft.geosites.sort();
        draft.geosites.dedup();
        self.ui
            .pac_selected_domains
            .retain(|selected| draft.domains.iter().any(|d| d == selected));
        self.commit_pac_list_change_for(profile_id, config_json, draft, endpoint);
        self.message = format!("已整理并按域名排序（去除 {removed_dups} 条重复）。");
    }

    fn pac_geosite_section(
        &mut self,
        ui: &mut egui::Ui,
        profile_id: &str,
        config_json: &str,
        draft: &mut PacDraft,
        endpoint: Option<std::net::SocketAddr>,
        cached_geo: Option<&GeoDataCache>,
    ) -> bool {
        let categories = cached_geo.map(|c| c.categories.as_slice()).unwrap_or(&[]);
        let geosite_error = cached_geo.and_then(|c| c.geosite_error.as_deref());
        let mut changed = false;
        ui.add_space(6.0);
        ui.label(RichText::new("GeoSite 分组").strong());

        if categories.is_empty() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                geosite_error.unwrap_or(
                    "未检测到 geosite.dat（可在「设置 · Core 与依赖」下载或配置资源目录）。",
                ),
            );
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("常用速选：").small().weak());
                for (name, title) in QUICK_GEOSITES {
                    let Some(cat) = categories.iter().find(|c| c.name == *name) else {
                        continue;
                    };
                    let spec = format!("geosite:{name}");
                    let selected = draft.geosites.iter().any(|g| g.eq_ignore_ascii_case(&spec));
                    let resp = ui
                        .selectable_label(selected, format!("{title} ({name})"))
                        .on_hover_text(format!(
                            "{spec} · 可导出 {} 条域名（含 {} 条规则总数）",
                            cat.pac_compatible(),
                            cat.total()
                        ));
                    if resp.clicked() {
                        if selected {
                            draft.geosites.retain(|g| !g.eq_ignore_ascii_case(&spec));
                        } else {
                            draft.geosites.push(spec);
                        }
                        self.ui.pac_compile_key = None;
                        changed = true;
                    }
                }
            });
        }

        if !draft.geosites.is_empty() {
            ui.add_space(4.0);
            let mut remove_spec = None;
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("已选分组：").small().weak());
                for spec in &draft.geosites {
                    if ui
                        .button(format!("{spec} ×"))
                        .on_hover_text("点击移除此 GeoSite 分组")
                        .clicked()
                    {
                        remove_spec = Some(spec.clone());
                    }
                }
            });
            if let Some(target) = remove_spec {
                draft.geosites.retain(|g| g != &target);
                self.ui.pac_compile_key = None;
                changed = true;
            }
        }

        if !categories.is_empty() {
            egui::CollapsingHeader::new(format!(
                "浏览 geosite.dat 全部分组（{} 类）",
                categories.len()
            ))
            .id_salt("pac-geosite-browser")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("搜索分组");
                    crate::widgets::singleline_select_all(
                        ui,
                        &mut self.ui.pac_geosite_search,
                        f32::INFINITY,
                        "输入关键词过滤，如 apple / steam / google / cn",
                    );
                });
                let needle = self.ui.pac_geosite_search.trim().to_ascii_lowercase();
                let matches: Vec<&GeoSiteCategorySummary> = categories
                    .iter()
                    .filter(|c| needle.is_empty() || c.name.contains(&needle))
                    .collect();
                let row_height = 26.0;
                egui::ScrollArea::vertical()
                    .id_salt("pac-geosite-scroll")
                    .max_height(180.0)
                    .auto_shrink([false, true])
                    .show_rows(ui, row_height, matches.len(), |ui, range| {
                        for cat in &matches[range] {
                            let spec = format!("geosite:{}", cat.name);
                            let mut checked =
                                draft.geosites.iter().any(|g| g.eq_ignore_ascii_case(&spec));
                            let label = if cat.regex_count > 0 {
                                format!(
                                    "{}  ({} 域名 · 忽略 {} 正则)",
                                    cat.name,
                                    cat.pac_compatible(),
                                    cat.regex_count
                                )
                            } else {
                                format!("{}  ({} 域名)", cat.name, cat.pac_compatible())
                            };
                            if ui.checkbox(&mut checked, label).changed() {
                                if checked {
                                    if !draft.geosites.iter().any(|g| g.eq_ignore_ascii_case(&spec))
                                    {
                                        draft.geosites.push(spec);
                                    }
                                } else {
                                    draft.geosites.retain(|g| !g.eq_ignore_ascii_case(&spec));
                                }
                                self.ui.pac_compile_key = None;
                                changed = true;
                            }
                        }
                    });
            });
        }
        if changed {
            self.commit_pac_list_change_for(profile_id, config_json, draft, endpoint);
        }
        changed
    }

    fn pac_domains_section(
        &mut self,
        ui: &mut egui::Ui,
        profile_id: &str,
        config_json: &str,
        draft: &mut PacDraft,
        endpoint: Option<std::net::SocketAddr>,
    ) {
        ui.label(RichText::new("2 · 自定义域名清单").strong().size(18.0));
        ui.label(
            RichText::new(
                "可输入单条域名、geosite:分组（支持 @attr），或直接粘贴多行/逗号分隔清单，回车即保存。",
            )
            .small()
            .weak(),
        );
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            let input_width = (ui.available_width() - 88.0).max(160.0);
            let response = crate::widgets::singleline_select_all(
                ui,
                &mut self.ui.pac_domain_input,
                input_width,
                "example.com 或 *.example.com / full:exact.com / geosite:cn",
            );
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let clicked = ui.button("添加").clicked();
            if enter || clicked {
                let added = self.add_pac_domain_for(profile_id, config_json, draft, endpoint);
                response.request_focus();
                if !added
                    && !self.ui.pac_domain_input.is_empty()
                    && let Some(state) = egui::TextEdit::load_state(ui.ctx(), response.id)
                {
                    crate::widgets::select_all_text(
                        ui.ctx(),
                        state,
                        response.id,
                        self.ui.pac_domain_input.chars().count(),
                    );
                }
            }
        });

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!profile_id.is_empty(), egui::Button::new("从路由规则提取"))
                .on_hover_text("从当前联动配置的 /routing/rules 提取普通域名与 geosite 分组")
                .clicked()
            {
                self.extract_pac_domains_from_config(profile_id, config_json, draft, endpoint);
            }
            if ui.button("批量粘贴…").clicked() {
                self.ui.pac_batch_input = if draft.domains.is_empty() {
                    String::new()
                } else {
                    proxy_pac::format_domain_list(&draft.domains)
                };
                self.ui.pac_batch_report = None;
                self.ui.pac_batch_replace = false;
                self.ui.dialog = Some(Dialog::PacBatch);
            }
            if ui
                .add_enabled(
                    !draft.domains.is_empty() || !draft.geosites.is_empty(),
                    egui::Button::new("整理去重"),
                )
                .clicked()
            {
                self.dedup_and_sort_pac_domains(profile_id, config_json, draft, endpoint);
            }
            if ui
                .add_enabled(!draft.domains.is_empty(), egui::Button::new("复制清单"))
                .clicked()
            {
                ui.ctx()
                    .copy_text(proxy_pac::format_domain_list(&draft.domains));
                self.message = format!("已复制 {} 条域名到剪贴板。", draft.domains.len());
            }
        });

        if draft.domains.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "自定义域名清单为空；可直接选择上方 GeoSite 分组，或手动添加额外域名。",
                )
                .small()
                .weak(),
            );
            return;
        }

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("筛选");
            crate::widgets::singleline_select_all(
                ui,
                &mut self.ui.pac_domain_filter,
                210.0,
                "按域名搜索",
            );
            if !self.ui.pac_domain_filter.is_empty() && ui.button("清除").clicked() {
                self.ui.pac_domain_filter.clear();
            }
            for (kind, label) in [
                (PacDomainFilter::All, "全部"),
                (PacDomainFilter::DomainAndSub, "含主域"),
                (PacDomainFilter::Subdomain, "仅子域"),
                (PacDomainFilter::Exact, "精确"),
            ] {
                ui.selectable_value(&mut self.ui.pac_domain_kind_filter, kind, label);
            }
        });

        let needle = self.ui.pac_domain_filter.trim().to_ascii_lowercase();
        let filtered: Vec<(usize, String, PacDomainKind)> = draft
            .domains
            .iter()
            .enumerate()
            .filter_map(|(idx, domain)| {
                let kind = proxy_pac::domain_kind(domain);
                let kind_ok = match self.ui.pac_domain_kind_filter {
                    PacDomainFilter::All => true,
                    PacDomainFilter::DomainAndSub => kind == PacDomainKind::DomainAndSub,
                    PacDomainFilter::Subdomain => kind == PacDomainKind::Subdomain,
                    PacDomainFilter::Exact => kind == PacDomainKind::Exact,
                };
                if !kind_ok {
                    return None;
                }
                if !needle.is_empty() && !domain.to_ascii_lowercase().contains(&needle) {
                    return None;
                }
                Some((idx, domain.clone(), kind))
            })
            .collect();

        let total = draft.domains.len();
        let shown = filtered.len();
        let selected_count = self
            .ui
            .pac_selected_domains
            .iter()
            .filter(|selected| draft.domains.iter().any(|d| d == *selected))
            .count();

        ui.horizontal_wrapped(|ui| {
            let stats = if shown == total {
                format!("共 {total} 条 · 已选 {selected_count} 条")
            } else {
                format!("共 {total} 条 · 匹配 {shown} 条 · 已选 {selected_count} 条")
            };
            ui.label(RichText::new(stats).small().weak());
            if ui
                .add_enabled(!filtered.is_empty(), egui::Button::new("全选当前"))
                .clicked()
            {
                for (_, domain, _) in &filtered {
                    self.ui.pac_selected_domains.insert(domain.clone());
                }
            }
            if ui
                .add_enabled(selected_count > 0, egui::Button::new("取消选择"))
                .clicked()
            {
                self.ui.pac_selected_domains.clear();
            }
            if ui
                .add_enabled(
                    selected_count > 0,
                    egui::Button::new(format!("移除所选 ({selected_count})")),
                )
                .clicked()
            {
                let before = draft.domains.len();
                draft
                    .domains
                    .retain(|domain| !self.ui.pac_selected_domains.contains(domain));
                let removed = before.saturating_sub(draft.domains.len());
                self.ui.pac_selected_domains.clear();
                self.commit_pac_list_change_for(profile_id, config_json, draft, endpoint);
                self.message = format!("已批量移除 {removed} 条域名。");
                return;
            }
            if ui.button("清空清单").clicked() {
                self.ui.dialog = Some(Dialog::Confirm(Action::ClearPacDomains(
                    profile_id.to_owned(),
                )));
            }
        });

        if filtered.is_empty() {
            ui.label(RichText::new("没有匹配当前筛选条件的域名。").small().weak());
            return;
        }

        let mut remove_index = None;
        let row_height = 28.0;
        egui::Frame::new()
            .stroke(egui::Stroke::new(
                1.0,
                ui.visuals().widgets.noninteractive.bg_stroke.color,
            ))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("pac-domain-list")
                    .max_height(240.0)
                    .auto_shrink([false, true])
                    .show_rows(ui, row_height, filtered.len(), |ui, range| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for (orig_index, domain, kind) in &filtered[range] {
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), 26.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let mut checked = self.ui.pac_selected_domains.contains(domain);
                                    if ui.checkbox(&mut checked, "").changed() {
                                        if checked {
                                            self.ui.pac_selected_domains.insert(domain.clone());
                                        } else {
                                            self.ui.pac_selected_domains.remove(domain);
                                        }
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if crate::widgets::compact_button(ui, "移除").clicked()
                                            {
                                                remove_index = Some(*orig_index);
                                            }
                                            ui.label(
                                                RichText::new(kind_label(*kind)).small().weak(),
                                            );
                                            ui.with_layout(
                                                egui::Layout::left_to_right(egui::Align::Center),
                                                |ui| {
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(domain).monospace(),
                                                        )
                                                        .truncate(),
                                                    )
                                                    .on_hover_text(domain);
                                                },
                                            );
                                        },
                                    );
                                },
                            );
                        }
                    });
            });

        if let Some(index) = remove_index {
            let removed = draft.domains.remove(index);
            self.ui.pac_selected_domains.remove(&removed);
            self.commit_pac_list_change_for(profile_id, config_json, draft, endpoint);
            self.message = format!("已移除域名：{removed}");
        }
    }

    fn active_pac_banner(&mut self, ui: &mut egui::Ui, target_profile: &str, draft: &PacDraft) {
        let running = self.is_running();
        let syncing = self.is_pac_sync_pending();
        let owned = self
            .status
            .as_ref()
            .is_some_and(|s| s.proxy_state == "Owned");
        let generation = self.status.as_ref().and_then(|s| s.generation.clone());
        let pac_serving = self
            .status
            .as_ref()
            .is_some_and(|s| s.pac_state == "Serving");
        let active_strategy = self.active_pac_strategy();

        card(ui, |ui| {
            ui.label(RichText::new("当前生效的 PAC 状态").strong().size(18.0));
            match (
                &generation,
                generation.as_ref().and_then(|g| g.pac.as_ref()),
            ) {
                (Some(g), Some(pac)) if g.proxy_mode == ProxyMode::Pac => {
                    let state_text = if running && owned && pac_serving {
                        "生效中 · PAC 自动代理正在接管系统代理"
                    } else if running && owned {
                        "已应用 PAC · 等待 PAC 服务就绪"
                    } else if running {
                        "Core 运行中（系统代理未由本程序接管）"
                    } else {
                        "Core 当前未运行（下列为上次应用的 PAC 版本）"
                    };
                    ui.label(RichText::new(state_text).strong().color(
                        if running && owned && pac_serving {
                            accent(ui)
                        } else {
                            ui.visuals().warn_fg_color
                        },
                    ));
                    let strategy_desc = match active_strategy {
                        Some((mode, Some(summary))) => {
                            format!("{}（{summary}）", Self::pac_mode_label(mode))
                        }
                        Some((mode, None)) => Self::pac_mode_label(mode).to_owned(),
                        None => format!(
                            "历史版本（修订 {}）",
                            &pac.revision[..pac.revision.len().min(8)]
                        ),
                    };
                    line(ui, "正在生效策略", &strategy_desc);
                    line(
                        ui,
                        "所属配置",
                        self.ui.workspace.name(&g.candidate.profile_id),
                    );
                    line(
                        ui,
                        "PAC 服务",
                        if pac_serving {
                            "已发布（正在监听本地端口）"
                        } else {
                            "未发布"
                        },
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("PAC URL").small().weak());
                        ui.monospace(&pac.url);
                        if ui.button("复制 URL").clicked() {
                            ui.ctx().copy_text(pac.url.clone());
                            self.message = "已复制正在生效的 PAC URL。".into();
                        }
                    });
                    let matches_draft = g.candidate.profile_id == target_profile
                        && pac.revision == hash(draft.script.as_bytes());
                    if matches_draft {
                        ui.label(
                            RichText::new("与当前激活配置及本页策略一致，无需重新应用。")
                                .small()
                                .color(accent(ui)),
                        );
                    } else if syncing {
                        ui.label(
                            RichText::new("正在自动同步更新生效中的 PAC 脚本…")
                                .small()
                                .color(accent(ui)),
                        );
                    } else {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            "注意：正在生效的 PAC 与本页当前编辑不一致；修改需点击下方「检查并应用 PAC」后才会生效。",
                        );
                    }
                    let pac_file = resolve_path(&self.root, &pac.path);
                    egui::CollapsingHeader::new("查看正在生效的 PAC 脚本内容")
                        .id_salt("active-pac-script-view")
                        .show(ui, |ui| {
                            if let Ok(content) = std::fs::read_to_string(&pac_file) {
                                let mut preview: String =
                                    content.lines().take(120).collect::<Vec<_>>().join("\n");
                                if content.lines().count() > 120 {
                                    preview.push_str("\n// ...（已截断，仅展示前 120 行）");
                                }
                                egui::ScrollArea::vertical()
                                    .id_salt("active-pac-script-scroll")
                                    .max_height(180.0)
                                    .show(ui, |ui| {
                                        ui.monospace(preview);
                                    });
                            } else {
                                ui.label(
                                    RichText::new("暂未读取到已落盘的 PAC 脚本文件。")
                                        .small()
                                        .weak(),
                                );
                            }
                        });
                }
                (Some(g), _) if running => {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!(
                            "未启用 PAC · 当前系统代理方式为「{}」",
                            mode_name(g.proxy_mode)
                        ),
                    );
                    ui.label(
                        RichText::new(
                            "在下方配置好策略后，点击「检查并应用 PAC」即可切换到 PAC 自动代理。",
                        )
                        .small()
                        .weak(),
                    );
                }
                _ => {
                    ui.label(RichText::new("当前未生效 PAC（Core 尚未启动）").weak());
                    ui.label(
                        RichText::new("配置好策略后点击「检查并应用 PAC」即可校验并启用。")
                            .small()
                            .weak(),
                    );
                }
            }
        });
        ui.add_space(12.0);
    }

    pub(in crate::app) fn pac_page(&mut self, ui: &mut egui::Ui) {
        heading(
            ui,
            "PAC 自动代理",
            "自动与当前激活配置联动；结合配置路由与 GeoSite 生成分流 PAC，修改策略实时检查并在 PAC 模式下自动热更新。",
        );
        let (target_profile, target_json) = self.active_pac_target();
        let mut draft = self.pac_draft_for(&target_profile);
        let mut changed = false;
        let has_config = !target_profile.is_empty() && !target_json.trim().is_empty();
        let endpoint_result = if has_config {
            let preferred = self.http_tag.clone();
            self.pac_endpoint_in(&target_json, Some(&preferred))
                .map(|(tag, ep)| {
                    if target_profile == self.profile_id {
                        self.http_tag = tag;
                    }
                    ep
                })
        } else {
            Err(String::new())
        };
        let endpoint = endpoint_result.as_ref().ok().copied();
        let cached_geo = self.refresh_geodata_cache_for(&target_json);
        if self.sync_pac_script_for(
            &target_profile,
            &target_json,
            &mut draft,
            endpoint,
            cached_geo.as_ref(),
        ) {
            changed = true;
        }

        self.active_pac_banner(ui, &target_profile, &draft);

        // Card 1: mode & GeoSite picker.
        card(ui, |ui| {
            ui.label(RichText::new("1 · 分流模式与 GeoSite").strong().size(18.0));
            if has_config {
                ui.horizontal_wrapped(|ui| {
                    line(ui, "联动配置", self.ui.workspace.name(&target_profile));
                    if target_profile != self.profile_id && self.committed_id().is_some() {
                        crate::widgets::badge(
                            ui,
                            "已自动跟随当前激活配置",
                            crate::widgets::BadgeTone::Accent,
                        );
                    }
                });
            }
            if let Some(endpoint) = endpoint {
                line(ui, "代理目标", &endpoint.to_string());
            } else if !has_config {
                ui.label(RichText::new("尚未选择配置；添加并检查配置后即可设置 PAC。").weak());
            } else {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    endpoint_result.as_ref().err().cloned().unwrap_or_else(|| {
                        "请先添加回环或 0.0.0.0 监听、无认证的 HTTP 或 Mixed 入站。".into()
                    }),
                );
            }

            ui.horizontal_wrapped(|ui| {
                ui.label("PAC 策略");
                egui::ComboBox::from_id_salt("pac-mode-select")
                    .width(280.0)
                    .selected_text(mode_label(draft.mode))
                    .show_ui(ui, |ui| {
                        for mode in [
                            PacMode::AutoRules,
                            PacMode::DirectList,
                            PacMode::ProxyList,
                            PacMode::All,
                            PacMode::Custom,
                        ] {
                            if ui
                                .selectable_value(&mut draft.mode, mode, mode_label(mode))
                                .clicked()
                            {
                                self.ui.pac_compile_key = None;
                                self.sync_pac_script_for(
                                    &target_profile,
                                    &target_json,
                                    &mut draft,
                                    endpoint,
                                    cached_geo.as_ref(),
                                );
                                self.save_pac_draft_for(&target_profile, draft.clone());
                                self.auto_sync_running_pac_if_active(&target_profile, &draft);
                                self.message = format!("已切换 PAC 策略：{}", mode_label(mode));
                                changed = false;
                            }
                        }
                    });
            });

            match draft.mode {
                PacMode::AutoRules => {
                    ui.label(
                        RichText::new(
                            "直接读取当前配置的「路由规则」，自动展开 geosite: 分组并折叠冗余子域，其他流量走 Xray 兜底。",
                        )
                        .small()
                        .weak(),
                    );
                }
                PacMode::DirectList => {
                    ui.label(
                        RichText::new(
                            "仅下列选中的 GeoSite 分组与自定义域名直连，其余流量进入 Xray。",
                        )
                        .small()
                        .weak(),
                    );
                }
                PacMode::ProxyList => {
                    ui.label(
                        RichText::new("仅下列选中的 GeoSite 分组与自定义域名走代理，其余直连。")
                            .small()
                            .weak(),
                    );
                }
                PacMode::All => {
                    ui.label(
                        RichText::new(
                            "所有请求（本地回环及 .lan/.local 局域网除外）统一进入 Xray。",
                        )
                        .small()
                        .weak(),
                    );
                }
                PacMode::Custom => {
                    ui.label(
                        RichText::new("使用下方「高级设置」中用户自定义的 JavaScript 脚本。")
                            .small()
                            .weak(),
                    );
                }
            }

            let stats = &self.ui.pac_compile_stats;
            let total_entries =
                stats.domain_count + stats.full_count + stats.subdomain_count + stats.keyword_count;
            if draft.mode != PacMode::All {
                let mut summary = format!(
                    "脚本体积：{:.1} KiB",
                    stats.script_bytes.max(draft.script.len()) as f64 / 1024.0
                );
                if total_entries > 0 || !stats.resolved_geosites.is_empty() {
                    summary = format!(
                        "已展开 {} 个 GeoSite · {} 条生效规则（含折叠 {} 条冗余子域）· {summary}",
                        stats.resolved_geosites.len(),
                        total_entries,
                        stats.folded_count
                    );
                }
                ui.label(RichText::new(summary).small().weak());
            }

            if matches!(draft.mode, PacMode::DirectList | PacMode::ProxyList) {
                changed |= self.pac_geosite_section(
                    ui,
                    &target_profile,
                    &target_json,
                    &mut draft,
                    endpoint,
                    cached_geo.as_ref(),
                );
            }
        });

        // Card 2: custom domain list (only when DirectList / ProxyList is active).
        if matches!(draft.mode, PacMode::DirectList | PacMode::ProxyList) {
            ui.add_space(12.0);
            card(ui, |ui| {
                self.pac_domains_section(ui, &target_profile, &target_json, &mut draft, endpoint);
            });
        }

        // Card 3: live status, test & apply.
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.label(RichText::new("测试与一键应用").strong().size(18.0));
            line(
                ui,
                "脚本状态",
                if draft.script.trim().is_empty() {
                    "等待有效配置入站"
                } else if self.ui.pac_checked {
                    "已通过自动语法检查"
                } else {
                    "语法校验未通过"
                },
            );
            if !self.ui.pac_notes.is_empty() {
                egui::CollapsingHeader::new(format!(
                    "编译与兼容提示（{} 条）",
                    self.ui.pac_notes.len()
                ))
                .default_open(!self.ui.pac_checked || self.ui.pac_notes.len() <= 2)
                .show(ui, |ui| {
                    for note in &self.ui.pac_notes {
                        ui.label(RichText::new(note).small().weak());
                    }
                });
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("测试 URL");
                let input_w = (ui.available_width() - 190.0).max(160.0);
                let resp = crate::widgets::singleline_select_all(
                    ui,
                    &mut self.ui.pac_test_url,
                    input_w,
                    "https://example.com/",
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let can_test = self.ui.connected && !draft.script.trim().is_empty() && !self.busy;
                if ui
                    .add_enabled(can_test, egui::Button::new("测试命中结果"))
                    .clicked()
                    || (enter && can_test)
                {
                    match proxy_pac::normalize_test_url(&self.ui.pac_test_url) {
                        Ok(url) => {
                            self.ui.pac_test_url = url.clone();
                            self.send(Command::TestPac {
                                script: draft.script.clone(),
                                url,
                            });
                        }
                        Err(error) => self.message = error.message,
                    }
                    crate::widgets::focus_and_select_all(ui.ctx(), &resp, &self.ui.pac_test_url);
                }
            });

            if let Ok(result) = serde_json::from_str::<serde_json::Value>(&self.ui.pac_test) {
                let decision = &result["decision"];
                if let (Some(kind), Some(raw)) =
                    (decision["kind"].as_str(), decision["raw"].as_str())
                {
                    line(ui, "最近测试结果", &format!("{kind} · {raw}"));
                }
                if let Some(notes) = result["notes"].as_array() {
                    for note in notes.iter().filter_map(|n| n.as_str()) {
                        ui.label(RichText::new(note).small().weak());
                    }
                }
            } else if !self.ui.pac_test.is_empty() {
                ui.monospace(&self.ui.pac_test);
            }

            ui.separator();
            if let Some(pac) = self
                .status
                .as_ref()
                .and_then(|s| s.generation.as_ref())
                .and_then(|g| g.pac.as_ref())
            {
                line(ui, "当前 PAC URL", &pac.url);
                if pac.revision == hash(draft.script.as_bytes()) {
                    ui.label(RichText::new("当前运行版本已同步此 PAC 脚本。").weak());
                } else if self.is_pac_sync_pending() {
                    ui.label(
                        RichText::new("正在自动同步最新 PAC 脚本到运行中的代理…")
                            .small()
                            .color(accent(ui)),
                    );
                } else {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "当前运行版本仍是旧脚本；点击下方按钮检查并应用新规则。",
                    );
                }
            } else {
                ui.label(RichText::new("当前运行版本未启用 PAC 模式。").weak());
            }

            if primary(
                ui,
                "检查并应用 PAC",
                endpoint.is_some()
                    && self.ui.pac_checked
                    && !self.busy
                    && self.ui.connected
                    && self.write_blocker().is_none(),
            ) {
                self.save_pac_draft_for(&target_profile, draft.clone());
                self.save_profile_proxy_mode(&target_profile, ProxyMode::Pac);
                if target_profile == self.profile_id {
                    self.mode = ProxyMode::Pac;
                    self.validate_and_apply_draft();
                } else {
                    let http_tag = self.runtime_http_tag();
                    self.send(Command::SetProxyMode {
                        mode: ProxyMode::Pac,
                        http_inbound_tag: http_tag,
                        pac: Some(draft.clone()),
                    });
                }
                changed = false;
            }
        });

        // Collapsible Advanced settings.
        ui.add_space(12.0);
        egui::CollapsingHeader::new("高级设置（端口、导入、自定义脚本）").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("PAC 服务端口");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut draft.port)
                            .range(1..=65535)
                            .speed(1),
                    )
                    .changed();
                ui.label(RichText::new("默认 18080，仅监听 127.0.0.1").small().weak());
            });
            ui.horizontal_wrapped(|ui| {
                #[cfg(windows)]
                if ui
                    .add_enabled(
                        !target_profile.is_empty() && self.write_blocker().is_none(),
                        egui::Button::new("导入 PAC 文件…"),
                    )
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("PAC / JavaScript", &["pac", "js"])
                        .pick_file()
                {
                    match std::fs::read_to_string(path) {
                        Ok(script) => {
                            draft.script = script;
                            draft.mode = PacMode::Custom;
                            self.ui.pac_compile_key = None;
                            self.sync_pac_script_for(
                                &target_profile,
                                &target_json,
                                &mut draft,
                                endpoint,
                                cached_geo.as_ref(),
                            );
                            changed = true;
                        }
                        Err(error) => self.message = format!("读取 PAC 文件失败：{error}"),
                    }
                }
                if draft.mode != PacMode::AutoRules && ui.button("恢复自动路由生成").clicked()
                {
                    draft.mode = PacMode::AutoRules;
                    self.ui.pac_compile_key = None;
                    self.sync_pac_script_for(
                        &target_profile,
                        &target_json,
                        &mut draft,
                        endpoint,
                        cached_geo.as_ref(),
                    );
                    changed = true;
                }
            });
            ui.label(
                RichText::new(
                    "直接编辑脚本会自动切换到「自定义脚本」模式；自定义脚本在独立沙盒评估。",
                )
                .small()
                .weak(),
            );
            let response = egui::ScrollArea::vertical()
                .id_salt("pac-script")
                .max_height(320.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut draft.script)
                            .code_editor()
                            .desired_rows(14)
                            .desired_width(f32::INFINITY),
                    )
                })
                .inner;
            if response.changed() {
                draft.mode = PacMode::Custom;
                self.ui.pac_compile_key = None;
                self.sync_pac_script_for(
                    &target_profile,
                    &target_json,
                    &mut draft,
                    endpoint,
                    cached_geo.as_ref(),
                );
                changed = true;
            }
        });
        if changed {
            self.save_pac_draft_for(&target_profile, draft.clone());
            self.auto_sync_running_pac_if_active(&target_profile, &draft);
        }
    }
}
