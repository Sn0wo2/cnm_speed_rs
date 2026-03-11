use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Province {
    pub code: &'static str,
    pub name: &'static str,
}

pub const PROVINCES: &[Province] = &[
    Province {
        code: "zj",
        name: "浙江",
    },
    Province {
        code: "js",
        name: "江苏",
    },
    Province {
        code: "gd",
        name: "广东",
    },
    Province {
        code: "sd",
        name: "山东",
    },
    Province {
        code: "sc",
        name: "四川",
    },
    Province {
        code: "hn",
        name: "河南",
    },
    Province {
        code: "hb",
        name: "湖北",
    },
    Province {
        code: "ah",
        name: "安徽",
    },
    Province {
        code: "fj",
        name: "福建",
    },
    Province {
        code: "sx",
        name: "山西",
    },
    Province {
        code: "ln",
        name: "辽宁",
    },
    Province {
        code: "jl",
        name: "吉林",
    },
    Province {
        code: "hlj",
        name: "黑龙江",
    },
    Province {
        code: "gx",
        name: "广西",
    },
    Province {
        code: "yn",
        name: "云南",
    },
    Province {
        code: "gz",
        name: "贵州",
    },
    Province {
        code: "shaanx",
        name: "陕西",
    },
    Province {
        code: "gs",
        name: "甘肃",
    },
    Province {
        code: "qh",
        name: "青海",
    },
    Province {
        code: "nx",
        name: "宁夏",
    },
    Province {
        code: "xj",
        name: "新疆",
    },
    Province {
        code: "nm",
        name: "内蒙古",
    },
    Province {
        code: "bj",
        name: "北京",
    },
    Province {
        code: "sh",
        name: "上海",
    },
    Province {
        code: "tj",
        name: "天津",
    },
    Province {
        code: "cq",
        name: "重庆",
    },
];

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginTestData {
    pub task_id: String,
    #[serde(default)]
    pub node_ip: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub node_id: String,
    #[serde(default)]
    pub node_name: String,
    #[serde(default, alias = "isTenThousand", alias = "isWanZhao")]
    pub is_ten_thousand: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiResponse {
    pub code: i32,
    pub data: serde_json::Value,
}
