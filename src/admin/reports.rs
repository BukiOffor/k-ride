use super::*;
fn days(period: Option<&str>) -> Result<i32, AdminError> {
    match period.unwrap_or("week") {
        "week" => Ok(7),
        "month" => Ok(30),
        "quarter" => Ok(90),
        _ => Err(bad("period must be week, month, or quarter")),
    }
}
pub(super) const REPORT_SQL: &str = r#"
WITH periods AS (SELECT 0 AS n,now() AT TIME ZONE 'UTC' AS stop UNION ALL SELECT 1,now() AT TIME ZONE 'UTC'-make_interval(days=>($1->>'days')::int)),
ride_stats AS (
 SELECT p.n,count(r.id)::float8 AS requested,count(r.id) FILTER(WHERE r.status='COMPLETED')::float8 AS completed,
 count(r.id) FILTER(WHERE r.assigned_at IS NOT NULL)::float8 AS matched,count(r.id) FILTER(WHERE r.status='CANCELLED')::float8 AS cancelled,
 count(DISTINCT r.passenger_id)::float8 AS passengers,count(DISTINCT r.driver_id)::float8 AS drivers
 FROM periods p LEFT JOIN rides r ON r.created_at>=p.stop-make_interval(days=>($1->>'days')::int) AND r.created_at<p.stop GROUP BY p.n
), verification AS (
 SELECT p.n,100.0*count(dp.id) FILTER(WHERE upper(dp.status)='APPROVED')/NULLIF(count(dp.id) FILTER(WHERE upper(dp.status) IN ('APPROVED','REJECTED')),0) AS approval,
 avg(extract(epoch FROM dp.updated_at-dp.created_at)/86400.0) FILTER(WHERE upper(dp.status)='APPROVED') AS verify_days
 FROM periods p LEFT JOIN driver_profiles dp ON dp.updated_at>=p.stop-make_interval(days=>($1->>'days')::int) AND dp.updated_at<p.stop GROUP BY p.n
), safety AS (
 SELECT p.n,count(i.id)::float8 AS incidents,count(i.id) FILTER(WHERE i.category='SOS_ACTIVATION')::float8 AS sos,
 avg(extract(epoch FROM i.resolved_at-i.opened_at)/60.0) FILTER(WHERE i.resolved_at IS NOT NULL) AS resolution
 FROM periods p LEFT JOIN admin_incidents i ON i.opened_at>=p.stop-make_interval(days=>($1->>'days')::int) AND i.opened_at<p.stop GROUP BY p.n
), rating AS (
 SELECT p.n,avg(rr.stars) FILTER(WHERE rr.rater_role='passenger') AS driver_rating,avg(rr.stars) FILTER(WHERE rr.rater_role='driver') AS passenger_rating
 FROM periods p LEFT JOIN ride_ratings rr ON rr.created_at>=p.stop-make_interval(days=>($1->>'days')::int) AND rr.created_at<p.stop GROUP BY p.n
), retention AS (
 SELECT n,100.0*count(*) FILTER(WHERE trips>1)/NULLIF(count(*),0) AS repeat FROM (
 SELECT p.n,r.passenger_id,count(*) AS trips FROM periods p JOIN rides r ON r.created_at>=p.stop-make_interval(days=>($1->>'days')::int) AND r.created_at<p.stop AND r.status='COMPLETED' GROUP BY p.n,r.passenger_id
 ) x GROUP BY n
), metrics AS (
 SELECT r.n,m.section,m.id,m.value::float8 FROM ride_stats r JOIN verification v USING(n) JOIN safety s USING(n) JOIN rating rt USING(n) LEFT JOIN retention re USING(n)
 CROSS JOIN LATERAL (VALUES
 ('adoption','rides_requested',r.requested),('adoption','rides_completed',r.completed),('adoption','active_passengers',r.passengers),('adoption','active_drivers',r.drivers),
 ('verification','approval_rate',v.approval),('verification','avg_verification_days',v.verify_days),
 ('performance','match_rate',100.0*r.matched/NULLIF(r.requested,0)),('performance','completion_rate',100.0*r.completed/NULLIF(r.requested,0)),('performance','cancellation_rate',100.0*r.cancelled/NULLIF(r.requested,0)),
 ('safety','incidents',s.incidents),('safety','sos_alerts',s.sos),('safety','avg_resolution_minutes',s.resolution),
 ('ratings','passenger_rating',rt.passenger_rating),('ratings','driver_rating',rt.driver_rating),('retention','repeat_passenger_rate',re.repeat)
 ) m(section,id,value)
), sections AS (
 SELECT cur.section,jsonb_agg(jsonb_build_object('id',cur.id,'value',cur.value,'deltaPercent',CASE WHEN prev.value<>0 THEN 100.0*(cur.value-prev.value)/prev.value END) ORDER BY cur.id) AS metrics
 FROM metrics cur LEFT JOIN metrics prev ON prev.id=cur.id AND prev.n=1 WHERE cur.n=0 GROUP BY cur.section
)
SELECT COALESCE(jsonb_agg(jsonb_build_object('id',section,'metrics',metrics) ORDER BY section),'[]'::jsonb) AS data FROM sections WHERE $2 IS NOT NULL
"#;
pub(super) const SERIES_SQL: &str = r#"
WITH days AS (SELECT generate_series((CURRENT_DATE-($1->>'days')::int+1)::timestamp,CURRENT_DATE::timestamp,'1 day') AS day),
r AS (SELECT created_at::date AS day,count(*) AS requested,count(*) FILTER(WHERE status='COMPLETED') AS completed,count(*) FILTER(WHERE assigned_at IS NOT NULL) AS matched FROM rides WHERE created_at>=CURRENT_DATE-($1->>'days')::int+1 GROUP BY 1),
v AS (SELECT updated_at::date AS day,avg(extract(epoch FROM updated_at-created_at)/86400.0) AS days FROM driver_profiles WHERE status='APPROVED' AND updated_at>=CURRENT_DATE-($1->>'days')::int+1 GROUP BY 1),
s AS (SELECT opened_at::date AS day,count(*) AS incidents FROM admin_incidents WHERE opened_at>=CURRENT_DATE-($1->>'days')::int+1 GROUP BY 1),
rt AS (SELECT created_at::date AS day,avg(stars) AS rating FROM ride_ratings WHERE created_at>=CURRENT_DATE-($1->>'days')::int+1 GROUP BY 1),
series AS (SELECT x.section,jsonb_agg(jsonb_build_object('label',to_char(d.day,'YYYY-MM-DD'),'value',x.value,'secondary',x.secondary) ORDER BY d.day) AS points FROM days d LEFT JOIN r ON r.day=d.day::date LEFT JOIN v ON v.day=d.day::date LEFT JOIN s ON s.day=d.day::date LEFT JOIN rt ON rt.day=d.day::date CROSS JOIN LATERAL (VALUES ('adoption',COALESCE(r.requested,0)::numeric,COALESCE(r.completed,0)::numeric),('verification',v.days,NULL),('performance',100.0*r.matched/NULLIF(r.requested,0),NULL),('safety',COALESCE(s.incidents,0)::numeric,NULL),('ratings',rt.rating,NULL)) x(section,value,secondary) GROUP BY x.section)
SELECT COALESCE(jsonb_object_agg(section,points),'{}'::jsonb) AS data FROM series WHERE $2 IS NOT NULL
"#;
async fn report_data(db: &mut Connection<'_>, d: i32, actor: Uuid) -> Result<Value, AdminError> {
    let mut sections = query(db, REPORT_SQL, json!({"days":d}), actor).await?;
    let series = query(db, SERIES_SQL, json!({"days":d}), actor).await?;
    if let Some(sections) = sections.as_array_mut() {
        for section in sections {
            let key = section["id"].as_str().unwrap_or("");
            if let Some(points) = series.get(key) {
                section["series"] = points.clone();
            }
        }
    }
    Ok(sections)
}

#[utoipa::path(get,path="/admin/reports",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=Vec<dto::ReportSection>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("period"=Option<String>,Query)))]
pub(super) async fn reports(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    let d = days(f.period.as_deref())?;
    let mut db = connection(&s).await?;
    Ok(Json(report_data(&mut db, d, c.user_id).await?))
}
#[utoipa::path(get,path="/admin/overview",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Overview),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("period"=Option<String>,Query)))]
pub(super) async fn overview(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    let d = days(f.period.as_deref())?;
    let mut db = connection(&s).await?;
    let r = report_data(&mut db, d, c.user_id).await?;
    let mut v=query(&mut db,r#"SELECT jsonb_build_object('primary',jsonb_build_array(
 jsonb_build_object('id','active_drivers','label','Active drivers','value',(SELECT count(*) FROM driver_availability a JOIN users u ON u.id=a.driver_id WHERE a.status IN ('ONLINE','ON_TRIP') AND u.is_active)),
 jsonb_build_object('id','live_rides','label','Live rides','value',(SELECT count(*) FROM rides WHERE status IN ('SEARCHING','DRIVER_ASSIGNED','DRIVER_EN_ROUTE','DRIVER_ARRIVING','DRIVER_ARRIVED','IN_PROGRESS','PAUSED'))),
 jsonb_build_object('id','pending_verification','label','Pending verification','value',(SELECT count(*) FROM driver_profiles WHERE upper(status) NOT IN ('DRAFT','APPROVED','VERIFIED','ACTIVE','REJECTED'))),
 jsonb_build_object('id','open_incidents','label','Open incidents','value',(SELECT count(*) FROM admin_incidents WHERE status<>'RESOLVED'))),
 'activeSos',(SELECT data FROM admin_sos_view WHERE status IN ('sent','acknowledged','responding') ORDER BY created_at DESC,id DESC LIMIT 1)) AS data WHERE $1 IS NOT NULL AND $2 IS NOT NULL"#,json!({}),c.user_id).await?;
    let wanted = [
        ("completion_rate", "completion_rate"),
        ("match_rate", "match_rate"),
        ("driver_rating", "avg_rating"),
        ("rides_completed", "rides_this_week"),
    ];
    let mut metrics = Vec::new();
    for (source, dest) in wanted {
        for section in r.as_array().unwrap() {
            for m in section["metrics"].as_array().unwrap() {
                if m["id"] == source {
                    let mut m = m.clone();
                    m["id"] = json!(dest);
                    metrics.push(m);
                }
            }
        }
    }
    v["secondary"] = json!(metrics);
    Ok(Json(v))
}
#[utoipa::path(get,path="/admin/reports/export",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="CSV download",body=String,content_type="text/csv"),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("period"=Option<String>,Query)))]
pub(super) async fn export(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Response, AdminError> {
    require_admin(&c)?;
    let d = days(f.period.as_deref())?;
    let mut db = connection(&s).await?;
    let sections = report_data(&mut db, d, c.user_id).await?;
    let mut csv = String::from("section,metric,value,deltaPercent\r\n");
    for section in sections.as_array().unwrap() {
        for m in section["metrics"].as_array().unwrap() {
            csv.push_str(&format!(
                "{},{},{},{}\r\n",
                section["id"].as_str().unwrap(),
                m["id"].as_str().unwrap(),
                m["value"]
                    .as_f64()
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
                m["deltaPercent"]
                    .as_f64()
                    .map(|n| n.to_string())
                    .unwrap_or_default()
            ));
        }
    }
    audit(
        &mut db,
        c.user_id,
        c.user_id,
        "reports.exported",
        json!({"days":d}),
    )
    .await?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=paid-rides-report.csv",
            ),
        ],
        csv,
    )
        .into_response())
}
