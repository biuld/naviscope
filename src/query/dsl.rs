use crate::model::graph::EdgeType;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum GraphQuery {
    /// 全局搜索符号 (Shell: grep)
    Grep {
        /// 搜索模式 (简单字符串或正则)
        pattern: String,
        /// 可选：按类型过滤，如 ["class", "method"]
        #[serde(default)]
        kind: Vec<String>,
        #[serde(default = "default_limit")]
        limit: usize,
    },

    /// 列出成员 (Shell: ls)
    Ls {
        /// 目标节点的 FQN，缺省则列出项目模块
        fqn: Option<String>,
        /// 可选：按类型过滤
        #[serde(default)]
        kind: Vec<String>,
    },

    /// 查看节点详细信息 (Shell: inspect)
    Inspect {
        /// 目标节点的 FQN
        fqn: String,
    },

    /// 追踪入向关系：调用者、实现者等 (Shell: callers)
    Incoming {
        fqn: String,
        /// 可选：过滤边类型
        #[serde(default)]
        edge_type: Vec<EdgeType>,
    },

    /// 追踪出向关系：被调用者、依赖等 (Shell: callees)
    Outgoing {
        fqn: String,
        /// 可选：过滤边类型
        #[serde(default)]
        edge_type: Vec<EdgeType>,
    },
}

fn default_limit() -> usize {
    20
}
