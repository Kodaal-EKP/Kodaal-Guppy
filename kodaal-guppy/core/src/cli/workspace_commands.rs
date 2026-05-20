fn projects(cli: &Cli, args: &ProjectsArgs) -> Result<i32, CliFailure> {
    match &args.command {
        Some(ProjectCommand::Show { id }) => project_show(cli, id),
        Some(ProjectCommand::Rename { id, name }) => {
            project_patch(cli, id, json!({ "name": name }))
        }
        Some(ProjectCommand::Color { id, color }) => {
            project_patch(cli, id, json!({ "color": color }))
        }
        Some(ProjectCommand::Delete { id, yes }) => project_delete(cli, id, *yes),
        Some(ProjectCommand::List) | None => projects_list(cli),
    }
}

fn projects_list(cli: &Cli) -> Result<i32, CliFailure> {
    let projects = client()?.get_json("/api/projects")?;
    if cli.json {
        print_json(&projects);
    } else if let Some(items) = projects.as_array() {
        println!("{:<38} {:<24} {:>8}", "ID", "NAME", "PROMPTS");
        for project in items {
            println!(
                "{:<38} {:<24} {:>8}",
                value_str(project, "id"),
                truncate(&value_str(project, "name"), 24),
                project
                    .get("prompt_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            );
        }
    }
    Ok(0)
}

fn project_show(cli: &Cli, id: &str) -> Result<i32, CliFailure> {
    let project = client()?.get_json(&format!("/api/projects/{}", query_encode(id)))?;
    if cli.json {
        print_json(&project);
    } else {
        println!("ID:      {}", value_str(&project, "id"));
        println!("Name:    {}", value_str(&project, "name"));
        println!("Path:    {}", value_str(&project, "path"));
        println!("Color:   {}", value_str(&project, "color"));
        println!(
            "Prompts: {}",
            project
                .get("prompt_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        );
    }
    Ok(0)
}

fn project_patch(cli: &Cli, id: &str, body: Value) -> Result<i32, CliFailure> {
    let project = client()?.patch_json(&format!("/api/projects/{}", query_encode(id)), body)?;
    if cli.json {
        print_json(&project);
    } else {
        println!("Updated project {}.", id);
    }
    Ok(0)
}

fn project_delete(cli: &Cli, id: &str, yes: bool) -> Result<i32, CliFailure> {
    if !yes {
        return Err(CliFailure {
            code: EXIT_BAD_USAGE,
            message: "project delete requires --yes; prompts are orphaned, not deleted".to_string(),
        });
    }
    client()?.delete(&format!("/api/projects/{}", query_encode(id)))?;
    if cli.json {
        print_json(&json!({ "deleted": true, "id": id }));
    } else {
        println!("Deleted project {id}; prompts were orphaned.");
    }
    Ok(0)
}

fn tags(cli: &Cli) -> Result<i32, CliFailure> {
    let tags = client()?.get_json("/api/tags")?;
    if cli.json {
        print_json(&tags);
    } else if let Some(items) = tags.as_array() {
        println!("{:<38} {:<24} {:>8}", "ID", "TAG", "PROMPTS");
        for tag in items {
            println!(
                "{:<38} {:<24} {:>8}",
                value_str(tag, "id"),
                truncate(&value_str(tag, "name"), 24),
                tag.get("count").and_then(Value::as_u64).unwrap_or(0)
            );
        }
    }
    Ok(0)
}

fn stats(cli: &Cli) -> Result<i32, CliFailure> {
    let stats = client()?.get_json("/api/stats")?;
    if cli.json {
        print_json(&stats);
    } else {
        println!(
            "Prompts:   {}",
            stats
                .get("total_prompts")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        );
        println!(
            "Projects:  {}",
            stats
                .get("total_projects")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        );
        println!(
            "Artifacts: {}",
            stats
                .get("total_artifacts")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        );
        if let Some(sources) = stats.get("by_source").and_then(Value::as_object) {
            for (source, count) in sources {
                println!("Source {source}: {}", count.as_i64().unwrap_or(0));
            }
        }
    }
    Ok(0)
}

fn blocklist(cli: &Cli, args: &BlocklistArgs) -> Result<i32, CliFailure> {
    let empty = args.add_domain.is_empty()
        && args.remove_domain.is_empty()
        && args.add_path.is_empty()
        && args.remove_path.is_empty()
        && args.add_source_app.is_empty()
        && args.remove_source_app.is_empty();
    let status = if empty {
        client()?.get_json("/api/capture/status")?
    } else {
        client()?.patch_json(
            "/api/capture/blocklist",
            json!({
                "domains": {"add": args.add_domain, "remove": args.remove_domain},
                "paths": {"add": args.add_path, "remove": args.remove_path},
                "source_apps": {"add": args.add_source_app, "remove": args.remove_source_app}
            }),
        )?
    };
    if cli.json {
        print_json(&status);
    } else {
        let blocklist = status.get("blocklist").cloned().unwrap_or(Value::Null);
        println!("Domains:     {}", join_json_array(blocklist.get("domains")));
        println!("Paths:       {}", join_json_array(blocklist.get("paths")));
        println!(
            "Source apps: {}",
            join_json_array(blocklist.get("source_apps"))
        );
    }
    Ok(0)
}

fn artifact(cli: &Cli, args: &ArtifactArgs) -> Result<i32, CliFailure> {
    match &args.command {
        ArtifactCommand::Attach {
            prompt_id,
            path,
            storage_mode,
        } => {
            let artifact = client()?.post_json(
                &format!("/api/prompts/{}/artifacts", query_encode(prompt_id)),
                json!({ "path": path.to_string_lossy(), "storage_mode": storage_mode }),
            )?;
            if cli.json {
                print_json(&artifact);
            } else {
                println!("Attached artifact {}.", value_str(&artifact, "id"));
            }
        }
        ArtifactCommand::Delete {
            prompt_id,
            artifact_id,
        } => {
            client()?.delete(&format!(
                "/api/prompts/{}/artifacts/{}",
                query_encode(prompt_id),
                query_encode(artifact_id)
            ))?;
            if cli.json {
                print_json(&json!({ "deleted": true, "id": artifact_id }));
            } else {
                println!("Deleted artifact {artifact_id}.");
            }
        }
        ArtifactCommand::Content {
            artifact_id,
            output,
        } => {
            let bytes = client()?.get_bytes(&format!(
                "/api/artifacts/{}/content",
                query_encode(artifact_id)
            ))?;
            if let Some(path) = output {
                fs::write(path, bytes).map_err(|error| CliFailure::generic(error.to_string()))?;
                println!("Wrote artifact content to {}.", path.display());
            } else {
                print!("{}", String::from_utf8_lossy(&bytes));
            }
        }
        ArtifactCommand::Copy {
            artifact_id,
            target_project_id,
            on_conflict,
        } => {
            let response = client()?.post_json(
                &format!("/api/artifacts/{}/copy", query_encode(artifact_id)),
                json!({ "target_project_id": target_project_id, "on_conflict": on_conflict }),
            )?;
            if cli.json {
                print_json(&response);
            } else {
                println!(
                    "Copied artifact to {}.",
                    value_str(&response, "target_path")
                );
            }
        }
        ArtifactCommand::Storage {
            artifact_id,
            storage_mode,
        } => {
            let response = client()?.patch_json(
                &format!("/api/artifacts/{}", query_encode(artifact_id)),
                json!({ "storage_mode": storage_mode }),
            )?;
            if cli.json {
                print_json(&response);
            } else {
                println!("Artifact {} storage is now {}.", artifact_id, storage_mode);
            }
        }
    }
    Ok(0)
}
