fn search(cli: &Cli, args: &SearchArgs) -> Result<i32, CliFailure> {
    let mut params = vec![
        ("q", args.query.clone()),
        ("limit", args.limit.to_string()),
        ("sort", args.sort.clone()),
    ];
    push_param(&mut params, "project_id", args.project.as_ref());
    push_param(&mut params, "source", args.source.as_ref());
    push_param(&mut params, "source_app", args.source_app.as_ref());
    push_param(&mut params, "from", args.from.as_ref());
    push_param(&mut params, "to", args.to.as_ref());
    if args.favorite {
        params.push(("favorite", "true".to_string()));
    }
    let response = client()?.get_json(&format!("/api/prompts?{}", encode_params(&params)))?;
    let items = response
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if cli.json {
        print_json(&Value::Array(items.clone()));
    } else {
        print_prompt_list(&items, args.full);
    }
    Ok(if items.is_empty() { EXIT_GENERIC } else { 0 })
}

fn suggest(cli: &Cli, args: &SuggestArgs) -> Result<i32, CliFailure> {
    let draft = args.draft.join(" ");
    if draft.trim().is_empty() {
        if !args.shell_hook {
            println!("No draft text provided.");
        }
        return Ok(0);
    }
    let mut params = vec![("q", draft), ("surface", args.source.clone())];
    push_param(&mut params, "source_app", args.source_app.as_ref());
    push_param(&mut params, "project_id", args.project.as_ref());
    if let Some(limit) = args.limit {
        params.push(("limit", limit.to_string()));
    }
    let response = client()?.get_json(&format!(
        "/api/prompts/suggestions?{}",
        encode_params(&params)
    ))?;
    if cli.json {
        print_json(&response);
        return Ok(0);
    }
    let enabled = response
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let items = response
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !enabled {
        if !args.shell_hook {
            println!("Smart suggestions are disabled in settings.");
        }
        return Ok(0);
    }
    if items.is_empty() {
        if !args.shell_hook {
            println!("No similar prompts.");
        }
        return Ok(0);
    }
    println!(
        "You have written {} similar prompt{} before:",
        response
            .get("similar_count")
            .and_then(Value::as_i64)
            .unwrap_or(items.len() as i64),
        if items.len() == 1 { "" } else { "s" }
    );
    for (index, prompt) in items.iter().enumerate() {
        let score = prompt.get("score").and_then(Value::as_f64).unwrap_or(0.0);
        println!(
            "{}. [{}/{}] score {:.3} {}",
            index + 1,
            value_str(prompt, "source"),
            value_str(prompt, "source_app"),
            score,
            value_str(prompt, "id")
        );
        println!("   {}", truncate(&value_str(prompt, "text"), 180));
    }
    Ok(0)
}

fn recent(cli: &Cli, args: &RecentArgs) -> Result<i32, CliFailure> {
    let limit = args.n.min(200);
    let response = client()?.get_json(&format!("/api/prompts?limit={limit}&sort=created_desc"))?;
    let items = response
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if cli.json {
        print_json(&Value::Array(items.clone()));
    } else {
        print_prompt_list(&items, false);
    }
    Ok(0)
}

fn show(cli: &Cli, id: &str) -> Result<i32, CliFailure> {
    let prompt = client()?.get_json(&format!("/api/prompts/{}", query_encode(id)))?;
    if cli.json {
        print_json(&prompt);
    } else {
        println!("ID:       {}", value_str(&prompt, "id"));
        println!("Project:  {}", value_str(&prompt, "project_name"));
        println!(
            "Source:   {} / {}",
            value_str(&prompt, "source"),
            value_str(&prompt, "source_app")
        );
        println!(
            "Favorite: {}",
            if prompt
                .get("favorite")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "yes"
            } else {
                "no"
            }
        );
        println!();
        println!("{}", value_str(&prompt, "text"));
    }
    Ok(0)
}

fn copy_prompt(cli: &Cli, id: &str) -> Result<i32, CliFailure> {
    let prompt = client()?.get_json(&format!("/api/prompts/{}", query_encode(id)))?;
    let text = value_str(&prompt, "text");
    write_clipboard(&text)?;
    let _ = client()?.post_json(
        &format!("/api/prompts/{}/reuse", query_encode(id)),
        json!({}),
    );
    if cli.json {
        print_json(&json!({ "copied": true, "id": id }));
    } else {
        println!("Copied prompt {id}.");
    }
    Ok(0)
}

fn delete_prompt(cli: &Cli, id: &str) -> Result<i32, CliFailure> {
    client()?.delete(&format!("/api/prompts/{}", query_encode(id)))?;
    if cli.json {
        print_json(&json!({ "deleted": true, "id": id }));
    } else {
        println!("Deleted prompt {id}.");
    }
    Ok(0)
}

fn favorite_prompt(cli: &Cli, args: &FavoriteArgs) -> Result<i32, CliFailure> {
    let prompt = client()?.patch_json(
        &format!("/api/prompts/{}", query_encode(&args.id)),
        json!({ "favorite": !args.unset }),
    )?;
    if cli.json {
        print_json(&prompt);
    } else {
        println!(
            "{} prompt {}.",
            if args.unset {
                "Unfavorited"
            } else {
                "Favorited"
            },
            args.id
        );
    }
    Ok(0)
}

fn tag_prompt(cli: &Cli, args: &TagArgs) -> Result<i32, CliFailure> {
    let prompt = client()?.post_json(
        &format!("/api/prompts/{}/tags", query_encode(&args.id)),
        json!({ "name": args.name }),
    )?;
    if cli.json {
        print_json(&prompt);
    } else {
        println!("Tagged prompt {} with #{}.", args.id, args.name);
    }
    Ok(0)
}

fn untag_prompt(cli: &Cli, args: &UntagArgs) -> Result<i32, CliFailure> {
    let tag_id = resolve_tag_id(&args.tag)?;
    client()?.delete(&format!(
        "/api/prompts/{}/tags/{}",
        query_encode(&args.id),
        query_encode(&tag_id)
    ))?;
    if cli.json {
        print_json(&json!({ "untagged": true, "id": args.id, "tag": args.tag }));
    } else {
        println!("Removed tag {} from prompt {}.", args.tag, args.id);
    }
    Ok(0)
}

fn prune_prompts(cli: &Cli, args: &PruneArgs) -> Result<i32, CliFailure> {
    if args.older_than.is_none()
        && args.shorter_than.is_none()
        && args.project.is_none()
        && args.source.is_none()
    {
        return Err(CliFailure {
            code: EXIT_BAD_USAGE,
            message: "prune requires at least one criterion".to_string(),
        });
    }
    if !args.dry_run && !args.yes {
        return Err(CliFailure {
            code: EXIT_BAD_USAGE,
            message: "destructive prune requires --yes; use --dry-run first to preview".to_string(),
        });
    }
    let result = client()?.post_json(
        "/api/prompts/prune",
        json!({
            "older_than": args.older_than,
            "shorter_than": args.shorter_than,
            "project_id": args.project,
            "source": args.source,
            "dry_run": args.dry_run
        }),
    )?;
    if cli.json {
        print_json(&result);
    } else {
        let count = result.get("deleted").and_then(Value::as_i64).unwrap_or(0);
        if args.dry_run {
            println!("{count} prompts would be deleted.");
        } else {
            println!("Deleted {count} prompts.");
        }
    }
    Ok(0)
}

fn import_data(cli: &Cli, args: &ImportArgs) -> Result<i32, CliFailure> {
    if !args.file.is_file() {
        return Err(CliFailure {
            code: EXIT_BAD_USAGE,
            message: format!("import file does not exist: {}", args.file.display()),
        });
    }
    let result = client()?.post_multipart_file("/api/import", "file", &args.file)?;
    if cli.json {
        print_json(&result);
    } else {
        let counts = result.get("imported").cloned().unwrap_or(Value::Null);
        println!(
            "Imported prompts={}, projects={}, tags={}, artifacts={}.",
            counts.get("prompts").and_then(Value::as_i64).unwrap_or(0),
            counts.get("projects").and_then(Value::as_i64).unwrap_or(0),
            counts.get("tags").and_then(Value::as_i64).unwrap_or(0),
            counts.get("artifacts").and_then(Value::as_i64).unwrap_or(0)
        );
    }
    Ok(0)
}
