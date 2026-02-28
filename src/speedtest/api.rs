use super::types::NodeInfo;
use super::SpeedTester;
use crate::source::cmcc_types::{ApiResponse, BeginTestData};

pub(super) struct DefaultNodeRequest<'a> {
    pub ip: &'a str,
    pub city: &'a str,
    pub account: &'a str,
    pub down_bw: i64,
    pub up_bw: i64,
    pub operator: &'a str,
    pub province: &'a str,
}

pub(super) struct BeginTestRequest<'a> {
    pub dbw: i64,
    pub ubw: i64,
    pub city: &'a str,
    pub user_ip: &'a str,
    pub province: &'a str,
    pub operator: &'a str,
    pub mode: &'a str,
    pub node_id: &'a str,
    pub is_sign_account: &'a str,
    pub bd_account: &'a str,
    pub is_use_plug: i32,
    pub network_type: &'a str,
    pub task_id: Option<&'a str>,
}

pub(super) struct SpeedtestApi<'a> {
    tester: &'a SpeedTester,
}

impl<'a> SpeedtestApi<'a> {
    pub(super) fn new(tester: &'a SpeedTester) -> Self {
        Self { tester }
    }

    pub(super) fn get_ip_info(
        &self,
        province: &str,
        city: &str,
        isp: &str,
        user_ip: &str,
    ) -> (i64, i64, String) {
        let payload = serde_json::json!({
            "province": province,
            "city": city,
            "isp": isp,
            "ip": user_ip,
            "shortName": city,
            "belongCity": city,
            "webProvince": province,
            "operator": isp,
        });

        if let Ok(mut resp) = self
            .tester
            .set_headers(self.tester.agent.post(&super::join_base(
                &self.tester.base_url,
                self.tester.endpoints.get_ip_info_path,
            )))
            .send_json(&payload)
        {
            if let Ok(json) = resp.body_mut().read_json::<ApiResponse>() {
                let dbw = json.data["downBandWidth"].as_f64().unwrap_or(0.0) as i64;
                let ubw = json.data["upBandWidth"].as_f64().unwrap_or(0.0) as i64;
                let account = json.data["account"].as_str().unwrap_or("-").to_string();
                return (dbw, ubw, account);
            }
        }

        (0, 0, "-".to_string())
    }

    pub(super) fn select_nodes_by_city(&self, city: &str) -> Vec<NodeInfo> {
        if let Ok(mut resp) = self
            .tester
            .set_headers(self.tester.agent.post(&super::join_base(
                &self.tester.base_url,
                self.tester.endpoints.select_node_by_city_path,
            )))
            .send_json(serde_json::json!({"city": city}))
        {
            if let Ok(json) = resp.body_mut().read_json::<ApiResponse>() {
                let mut nodes =
                    serde_json::from_value::<Vec<NodeInfo>>(json.data).unwrap_or_default();
                nodes.sort_by(|a, b| b.status.cmp(&a.status).then_with(|| a.id.cmp(&b.id)));
                return nodes;
            }
        }

        vec![]
    }

    pub(super) fn get_default_node(&self, req: &DefaultNodeRequest<'_>) -> Option<NodeInfo> {
        let payload = serde_json::json!({
            "ip": req.ip,
            "city": req.city,
            "account": req.account,
            "decryptAccount": "",
            "downBW": req.down_bw,
            "upBW": req.up_bw,
            "operator": req.operator,
            "province": req.province,
            "shortName": req.city,
            "belongCity": req.city,
            "isp": req.operator,
            "webProvince": req.province,
            "mode": "Down",
        });

        if let Ok(mut resp) = self
            .tester
            .set_headers(self.tester.agent.post(&super::join_base(
                &self.tester.base_url,
                self.tester.endpoints.get_default_node_path,
            )))
            .send_json(&payload)
        {
            if let Ok(json) = resp.body_mut().read_json::<ApiResponse>() {
                return serde_json::from_value(json.data).ok();
            }
        }

        None
    }

    pub(super) fn begin_test(&self, req: &BeginTestRequest<'_>) -> Option<BeginTestData> {
        let payload = serde_json::json!({
            "dbw": req.dbw,
            "ubw": req.ubw,
            "city": req.city,
            "userIp": req.user_ip,
            "province": req.province,
            "operator": req.operator,
            "mode": req.mode,
            "nodeId": req.node_id,
            "isSignAccount": req.is_sign_account,
            "bdAccount": req.bd_account,
            "isUsePlug": req.is_use_plug,
            "networkType": req.network_type,
            "taskId": req.task_id,
        });

        if let Ok(mut resp) = self
            .tester
            .set_headers(self.tester.agent.post(&super::join_base(
                &self.tester.base_url,
                self.tester.endpoints.begin_test_path,
            )))
            .send_json(&payload)
        {
            if let Ok(json) = resp.body_mut().read_json::<ApiResponse>() {
                return self.tester.parse_begin_test(json.data);
            }
        }

        None
    }
}
