//! Account function implementations for the BQL executor.

use crate::ast::FunctionCall;
use crate::error::QueryError;

use super::super::types::{PostingContext, Value};
use super::super::Executor;

impl<'a> Executor<'a> {
    /// Evaluate account functions: `PARENT`, `LEAF`, `ROOT`, `ACCOUNT_DEPTH`, `ACCOUNT_SORTKEY`.
    pub(crate) fn eval_account_function(
        &self,
        name: &str,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        match name {
            "PARENT" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => {
                        if let Some(idx) = s.rfind(':') {
                            Ok(Value::String(s[..idx].to_string()))
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    _ => Err(QueryError::Type(
                        "PARENT expects an account string".to_string(),
                    )),
                }
            }
            "LEAF" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => {
                        if let Some(idx) = s.rfind(':') {
                            Ok(Value::String(s[idx + 1..].to_string()))
                        } else {
                            Ok(Value::String(s))
                        }
                    }
                    _ => Err(QueryError::Type(
                        "LEAF expects an account string".to_string(),
                    )),
                }
            }
            "ROOT" => self.eval_root(func, ctx),
            "ACCOUNT_DEPTH" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => {
                        let depth = s.chars().filter(|c| *c == ':').count() + 1;
                        Ok(Value::Integer(depth as i64))
                    }
                    _ => Err(QueryError::Type(
                        "ACCOUNT_DEPTH expects an account string".to_string(),
                    )),
                }
            }
            "ACCOUNT_SORTKEY" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(s) => Ok(Value::String(s)),
                    _ => Err(QueryError::Type(
                        "ACCOUNT_SORTKEY expects an account string".to_string(),
                    )),
                }
            }
            _ => unreachable!(),
        }
    }

    /// Evaluate account metadata functions: `OPEN_DATE`, `CLOSE_DATE`, `OPEN_META`.
    pub(crate) fn eval_account_meta_function(
        &self,
        name: &str,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        match name {
            "OPEN_DATE" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(account) => {
                        if let Some(info) = self.account_info.get(&account) {
                            Ok(info.open_date.map_or(Value::Null, Value::Date))
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    _ => Err(QueryError::Type(
                        "OPEN_DATE expects an account string".to_string(),
                    )),
                }
            }
            "CLOSE_DATE" => {
                Self::require_args(name, func, 1)?;
                let val = self.evaluate_expr(&func.args[0], ctx)?;
                match val {
                    Value::String(account) => {
                        if let Some(info) = self.account_info.get(&account) {
                            Ok(info.close_date.map_or(Value::Null, Value::Date))
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    _ => Err(QueryError::Type(
                        "CLOSE_DATE expects an account string".to_string(),
                    )),
                }
            }
            "OPEN_META" => {
                Self::require_args(name, func, 2)?;
                let account_val = self.evaluate_expr(&func.args[0], ctx)?;
                let key_val = self.evaluate_expr(&func.args[1], ctx)?;

                let (account, key) = match (account_val, key_val) {
                    (Value::String(a), Value::String(k)) => (a, k),
                    _ => {
                        return Err(QueryError::Type(
                            "OPEN_META expects (account_string, key_string)".to_string(),
                        ));
                    }
                };

                if let Some(info) = self.account_info.get(&account) {
                    let meta_value = info.open_meta.get(&key);
                    Ok(Self::meta_value_to_value(meta_value))
                } else {
                    Ok(Value::Null)
                }
            }
            _ => unreachable!(),
        }
    }

    /// Evaluate ROOT function (takes 1-2 arguments).
    pub(crate) fn eval_root(
        &self,
        func: &FunctionCall,
        ctx: &PostingContext,
    ) -> Result<Value, QueryError> {
        if func.args.is_empty() || func.args.len() > 2 {
            return Err(QueryError::InvalidArguments(
                "ROOT".to_string(),
                "expected 1 or 2 arguments".to_string(),
            ));
        }

        let val = self.evaluate_expr(&func.args[0], ctx)?;
        let n = if func.args.len() == 2 {
            match self.evaluate_expr(&func.args[1], ctx)? {
                Value::Integer(i) => i as usize,
                _ => {
                    return Err(QueryError::Type(
                        "ROOT second arg must be integer".to_string(),
                    ));
                }
            }
        } else {
            1
        };

        match val {
            Value::String(s) => {
                let parts: Vec<&str> = s.split(':').collect();
                if n >= parts.len() {
                    Ok(Value::String(s))
                } else {
                    Ok(Value::String(parts[..n].join(":")))
                }
            }
            _ => Err(QueryError::Type(
                "ROOT expects an account string".to_string(),
            )),
        }
    }
}
