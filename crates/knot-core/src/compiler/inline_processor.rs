use crate::cache::Cache;
use crate::executors::r::RExecutor;
use crate::parser::InlineExpr;
use anyhow::{Context, Result};
use log::info;

pub fn process_inline_expr(
    inline_expr: &InlineExpr,
    r_executor: &mut Option<RExecutor>,
    cache: &mut Cache,
    previous_hash: &str,
) -> Result<(String, String)> {
    let inline_hash = cache.get_inline_expr_hash(
        &inline_expr.code,
        &inline_expr.verb,
        previous_hash,
    );

    if cache.has_cached_inline_result(&inline_hash) {
        info!("  ✓ #r[{}] [cached]", inline_expr.code);
        let cached_result = cache.get_cached_inline_result(&inline_hash)?;
        return Ok((cached_result, inline_hash));
    }

    let result_str = if inline_expr.verb.as_deref() == Some("run") {
        info!("  ⚙️ #r:run[{}] [executing for side effect]", inline_expr.code);
        r_executor
            .as_mut()
            .context("R executor not initialized")?
            .execute_side_effect_only(&inline_expr.code)
            .context(format!(
                "Failed to execute inline expression: #r:run[{}]",
                inline_expr.code
            ))?;
        String::new() // No output for 'run' verb
    } else {
        info!("  ⚙️ #r[{}] [executing]", inline_expr.code);
        let result = r_executor
            .as_mut()
            .context("R executor not initialized")?
            .execute_inline(&inline_expr.code)
            .context(format!(
                "Failed to execute inline expression: #r[{}]",
                inline_expr.code
            ))?;
        
        // Only cache if it's a value-producing inline (no verb)
        if inline_expr.verb.is_none() { 
            cache.save_inline_result(inline_hash.clone(), &result)?;
        }
        result
    };

    Ok((result_str, inline_hash))
}
