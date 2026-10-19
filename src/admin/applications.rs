use super::*;

const DOCS: &[&str] = &[
    "GOVERNMENT_ID",
    "PROFILE_PHOTO",
    "DRIVERS_LICENCE",
    "VEHICLE_REGISTRATION",
    "INSURANCE",
    "ROADWORTHINESS_CERTIFICATE",
    "VEHICLE_PHOTO",
    "GUARANTOR_PHOTO",
];
pub(super) const DOCUMENT_SQL: &str = "SELECT COALESCE(jsonb_agg(jsonb_build_object('type',d.kind,'label',replace(initcap(lower(d.kind)),'_',' '),'status',CASE WHEN d.body IS NULL OR d.body='' THEN 'MISSING' WHEN d.kind='DRIVERS_LICENCE' AND p.licence_expiry_date<CURRENT_DATE THEN 'EXPIRED' ELSE COALESCE(r.status,'UPLOADED') END,'base64',d.body,'note',r.note,'uploadedAt',p.updated_at AT TIME ZONE 'UTC','expiresAt',CASE WHEN d.kind='DRIVERS_LICENCE' THEN p.licence_expiry_date END) ORDER BY d.kind),'[]'::jsonb) AS data FROM driver_profiles p JOIN cars c ON c.id=p.car_id CROSS JOIN LATERAL (VALUES ('GOVERNMENT_ID',p.nin_slip),('PROFILE_PHOTO',p.profile_photo),('DRIVERS_LICENCE',p.licence_photo),('VEHICLE_REGISTRATION',c.vehicle_registration),('INSURANCE',c.insurance_certificate),('ROADWORTHINESS_CERTIFICATE',c.roadworthiness_certificate),('VEHICLE_PHOTO',c.vehicle_photo),('GUARANTOR_PHOTO',p.guarantor_photo)) d(kind,body) LEFT JOIN admin_document_reviews r ON r.user_id=p.user_id AND r.document_type=d.kind WHERE p.user_id=$2 AND (d.kind<>'GUARANTOR_PHOTO' OR NOT p.is_koinonia_worker) AND $1 IS NOT NULL";

#[utoipa::path(get,path="/admin/driver-applications",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::ApplicationSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("status"=Option<String>,Query),("track"=Option<String>,Query),("q"=Option<String>,Query),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
pub(super) async fn list(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    let source = "(SELECT p.id,p.data || jsonb_build_object('applicantName',p.data->'displayName','status',upper(dp.status),'submittedAt',dp.created_at AT TIME ZONE 'UTC','updatedAt',dp.updated_at AT TIME ZONE 'UTC','documentsReady',(SELECT count(*) FROM admin_document_reviews r WHERE r.user_id=p.id AND r.status='VERIFIED' AND (r.document_type<>'DRIVERS_LICENCE' OR dp.licence_expiry_date>=CURRENT_DATE) AND (r.document_type<>'GUARANTOR_PHOTO' OR NOT dp.is_koinonia_worker)),'documentsTotal',CASE WHEN dp.is_koinonia_worker THEN 7 ELSE 8 END) AS data,dp.track,upper(dp.status) AS status FROM admin_people_view p JOIN driver_profiles dp ON dp.user_id=p.id) applications";
    let mut result=query(&mut db,&page_sql(source,"($1->>'status' IS NULL OR status=$1->>'status') AND ($1->>'track' IS NULL OR track=$1->>'track' OR track='Both') AND ($1->>'q' IS NULL OR data->>'applicantName' ILIKE '%' || ($1->>'q') || '%')"),f.params()?,c.user_id).await?;
    result["counts"]=query(&mut db,"SELECT COALESCE(jsonb_object_agg(status,n),'{}'::jsonb) AS data FROM (SELECT upper(status) AS status,count(*) AS n FROM driver_profiles WHERE $1 IS NOT NULL AND $2 IS NOT NULL GROUP BY upper(status)) c",json!({}),c.user_id).await?;
    Ok(Json(result))
}
fn document_metadata(d: &mut Value) {
    let raw = d["base64"].as_str().unwrap_or("");
    let (mime, encoded) = if let Some((prefix, encoded)) = raw.split_once(",") {
        (
            prefix
                .strip_prefix("data:")
                .and_then(|s| s.strip_suffix(";base64"))
                .unwrap_or("application/octet-stream"),
            encoded,
        )
    } else {
        ("application/octet-stream", raw)
    };
    let mime = mime.to_owned();
    let size = encoded.len().saturating_mul(3) / 4
        - encoded
            .chars()
            .rev()
            .take_while(|c| *c == '=')
            .count()
            .min(encoded.len().saturating_mul(3) / 4);
    d["mimeType"] = json!(mime);
    d["sizeBytes"] = json!(size);
}
async fn record(db: &mut Connection<'_>, id: Uuid, applicant: bool) -> Result<Value, AdminError> {
    let mut v=query(db,"SELECT p.data || jsonb_build_object('applicantName',p.data->'displayName','status',upper(dp.status),'submittedAt',dp.created_at AT TIME ZONE 'UTC','updatedAt',dp.updated_at AT TIME ZONE 'UTC','licence',jsonb_build_object('number',dp.licence_number,'class',dp.licence_class,'expiryDate',dp.licence_expiry_date),'affiliation',jsonb_build_object('isKoinoniaWorker',dp.is_koinonia_worker,'department',dp.koinonia_department,'guarantor',dp.guarantor_name),'changesRequested',COALESCE(s.changes_requested,'[]'::jsonb)) AS data FROM admin_people_view p JOIN driver_profiles dp ON dp.user_id=p.id LEFT JOIN admin_user_state s ON s.user_id=p.id WHERE p.id=$2 AND $1 IS NOT NULL",json!({}),id).await?;
    v["documents"] = query(db, DOCUMENT_SQL, json!({}), id).await?;
    if let Some(ds) = v["documents"].as_array_mut() {
        for d in ds {
            document_metadata(d);
        }
    }
    v["documentsTotal"] = json!(v["documents"].as_array().map_or(0, Vec::len));
    v["documentsReady"] = json!(v["documents"].as_array().map_or(0, |ds| {
        ds.iter().filter(|d| d["status"] == "VERIFIED").count()
    }));
    v["comments"]=query(db,"SELECT COALESCE(jsonb_agg(jsonb_build_object('id',c.id,'body',c.body,'visibility',c.visibility,'documentType',c.document_type,'author',jsonb_build_object('userId',a.id,'displayName',a.first_name || ' ' || a.last_name),'at',c.created_at AT TIME ZONE 'UTC') ORDER BY c.created_at,c.id),'[]'::jsonb) AS data FROM admin_review_comments c JOIN users a ON a.id=c.author_id WHERE c.user_id=$2 AND (NOT ($1->>'applicant')::boolean OR c.visibility='APPLICANT')",json!({"applicant":applicant}),id).await?;
    Ok(v)
}
#[utoipa::path(get,path="/admin/driver-applications/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Application),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub(super) async fn detail(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    Ok(Json(record(&mut db, id, false).await?))
}
/// Driver-facing feedback deliberately excludes the internal review thread.
#[utoipa::path(get,path="/rides/me/driver-review",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")))]
pub async fn own_review(
    State(s): State<Arc<AppState>>,
    c: Claims,
) -> Result<Json<Value>, AdminError> {
    let mut db = connection(&s).await?;
    let v = record(&mut db, c.user_id, true).await?;
    Ok(Json(
        json!({"status":v["status"],"documents":v["documents"],"comments":v["comments"],"changesRequested":v["changesRequested"],"inspection":v["inspection"],"deactivationReason":v["deactivationReason"]}),
    ))
}
#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct DocumentReview {
    status: String,
    note: Option<String>,
}
#[utoipa::path(patch,path="/admin/driver-applications/{id}/documents/{doc}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::ApplicationDocument),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=DocumentReview,params(("id"=Uuid,Path),("doc"=String,Path)))]
pub(super) async fn document(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path((id, doc)): Path<(Uuid, String)>,
    Json(p): Json<DocumentReview>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    member(&doc, DOCS)?;
    member(&p.status, &["VERIFIED", "NEEDS_ATTENTION"])?;
    if p.status == "NEEDS_ATTENTION" {
        text(
            p.note
                .as_deref()
                .ok_or_else(|| bad("A reason is required"))?,
        )?;
    }
    let mut db = connection(&s).await?;
    let result=db.transaction::<Value,AdminError,_>(|db|Box::pin(async {
        query(db,"SELECT '{}'::jsonb AS data FROM driver_profiles p JOIN cars c ON c.id=p.car_id WHERE p.user_id=$2 AND $1 IS NOT NULL FOR UPDATE OF p,c",json!({}),id).await?;
        let docs=query(db,DOCUMENT_SQL,json!({}),id).await?;
        let current=docs.as_array().and_then(|ds|ds.iter().find(|d|d["type"]==doc)).ok_or_else(missing)?;
        if p.status=="VERIFIED" && (current["status"]=="MISSING"||current["status"]=="EXPIRED") {return Err(conflict("Missing or expired documents cannot be verified"));}
        let params=json!({"doc":doc,"status":p.status,"note":p.note,"reviewer":c.user_id});
        query(db,"INSERT INTO admin_document_reviews(user_id,document_type,status,note,reviewer_id) VALUES($2,$1->>'doc',$1->>'status',$1->>'note',($1->>'reviewer')::uuid) ON CONFLICT(user_id,document_type) DO UPDATE SET status=EXCLUDED.status,note=EXCLUDED.note,reviewer_id=EXCLUDED.reviewer_id,updated_at=(now() AT TIME ZONE 'UTC') RETURNING '{}'::jsonb AS data",params.clone(),id).await?;
        audit(db,c.user_id,id,"verification.document_reviewed",params).await?;
        let mut d=current.clone(); d["status"]=json!(p.status);d["note"]=json!(p.note);document_metadata(&mut d);Ok(d)
    })).await?;
    Ok(Json(result))
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct Comment {
    body: String,
    visibility: String,
    document_type: Option<String>,
}
#[utoipa::path(post,path="/admin/driver-applications/{id}/comments",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Comment,params(("id"=Uuid,Path)))]
pub(super) async fn comment(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Comment>,
) -> Result<Json<Value>, AdminError> {
    text(&p.body)?;
    member(&p.visibility, &["INTERNAL", "APPLICANT"])?;
    if let Some(ref d) = p.document_type {
        member(d, DOCS)?;
    }
    Ok(Json(mutate(&s,&c,id,"verification.comment_added","INSERT INTO admin_review_comments(id,user_id,author_id,body,visibility,document_type) SELECT ($1->>'id')::uuid,user_id,($1->>'author')::uuid,$1->>'body',$1->>'visibility',$1->>'doc' FROM driver_profiles WHERE user_id=$2 RETURNING jsonb_build_object('id',id,'body',body,'visibility',visibility,'documentType',document_type,'author',author_id,'at',created_at AT TIME ZONE 'UTC') AS data",json!({"id":Uuid::now_v7(),"author":c.user_id,"body":p.body,"visibility":p.visibility,"doc":p.document_type})).await?))
}
#[derive(Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct Inspection {
    scheduled_at: chrono::DateTime<chrono::Utc>,
    location: String,
    note: Option<String>,
}
async fn schedule(
    s: &AppState,
    c: &Claims,
    id: Uuid,
    p: Inspection,
    application: bool,
) -> Result<Json<Value>, AdminError> {
    text(&p.location)?;
    if p.scheduled_at <= chrono::Utc::now() {
        return Err(bad("Inspection must be scheduled in the future"));
    }
    let mut data = serde_json::to_value(p).unwrap();
    data["status"] = json!("SCHEDULED");
    let result=mutate(s,c,id,"inspection.scheduled","WITH valid AS (SELECT user_id FROM driver_profiles WHERE user_id=$2), st AS (INSERT INTO admin_user_state(user_id,inspection) SELECT user_id,$1->'inspection' FROM valid ON CONFLICT(user_id) DO UPDATE SET inspection=EXCLUDED.inspection,updated_at=(now() AT TIME ZONE 'UTC') RETURNING inspection), dp AS (UPDATE driver_profiles SET status=CASE WHEN ($1->>'application')::boolean THEN 'INSPECTION_SCHEDULED' ELSE status END,updated_at=(now() AT TIME ZONE 'UTC') WHERE user_id IN(SELECT user_id FROM valid)) SELECT inspection AS data FROM st",json!({"inspection":data,"application":application})).await?;
    Ok(Json(result))
}
#[utoipa::path(post,path="/admin/driver-applications/{id}/inspection",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Inspection,params(("id"=Uuid,Path)))]
pub(super) async fn inspection(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Inspection>,
) -> Result<Json<Value>, AdminError> {
    schedule(&s, &c, id, p, true).await
}
#[utoipa::path(post,path="/admin/drivers/{id}/inspection",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Inspection,params(("id"=Uuid,Path)))]
pub(super) async fn driver_inspection(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Inspection>,
) -> Result<Json<Value>, AdminError> {
    schedule(&s, &c, id, p, false).await
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct Decision {
    decision: String,
    reason: Option<String>,
    #[serde(default)]
    changes_requested: Vec<String>,
}
#[utoipa::path(post,path="/admin/driver-applications/{id}/decision",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Decision,params(("id"=Uuid,Path)))]
pub(super) async fn decision(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Decision>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    member(&p.decision, &["APPROVE", "REQUEST_CHANGES", "REJECT"])?;
    if p.decision != "APPROVE" {
        text(
            p.reason
                .as_deref()
                .ok_or_else(|| bad("A decision reason is required"))?,
        )?;
    }
    if p.decision == "REQUEST_CHANGES" && p.changes_requested.is_empty() {
        return Err(bad("changesRequested must list the required changes"));
    }
    let mut db = connection(&s).await?;
    let value = db
        .transaction::<Value, AdminError, _>(|db| Box::pin(apply_decision(db, &c, id, &p)))
        .await?;
    Ok(Json(value))
}
pub(super) async fn apply_decision(
    db: &mut Connection<'_>,
    c: &Claims,
    id: Uuid,
    p: &Decision,
) -> Result<Value, AdminError> {
    query(db,"SELECT '{}'::jsonb AS data FROM driver_profiles p JOIN cars c ON c.id=p.car_id WHERE p.user_id=$2 AND $1 IS NOT NULL FOR UPDATE OF p,c",json!({}),id).await?;
    if p.decision == "APPROVE" {
        let docs = query(db, DOCUMENT_SQL, json!({}), id).await?;
        if docs
            .as_array()
            .is_none_or(|ds| ds.is_empty() || ds.iter().any(|d| d["status"] != "VERIFIED"))
        {
            return Err(conflict(
                "All required documents must be verified before approval",
            ));
        }
    }
    let status = match p.decision.as_str() {
        "APPROVE" => "APPROVED",
        "REJECT" => "REJECTED",
        _ => "CHANGES_REQUIRED",
    };
    let params = json!({"status":status,"reason":p.reason,"changesRequested":p.changes_requested,"id":Uuid::now_v7(),"author":c.user_id});
    let result=query(db,"WITH p AS (UPDATE driver_profiles SET status=$1->>'status',updated_at=(now() AT TIME ZONE 'UTC') WHERE user_id=$2 RETURNING car_id), car AS (UPDATE cars SET is_verified=true,updated_at=(now() AT TIME ZONE 'UTC') WHERE id IN(SELECT car_id FROM p) AND $1->>'status'='APPROVED'), st AS (INSERT INTO admin_user_state(user_id,changes_requested) VALUES($2,$1->'changesRequested') ON CONFLICT(user_id) DO UPDATE SET changes_requested=EXCLUDED.changes_requested,updated_at=(now() AT TIME ZONE 'UTC')), cm AS (INSERT INTO admin_review_comments(id,user_id,author_id,body,visibility) SELECT ($1->>'id')::uuid,$2,($1->>'author')::uuid,$1->>'reason','APPLICANT' WHERE $1->>'reason' IS NOT NULL) SELECT jsonb_build_object('status',$1->>'status') AS data FROM p",params.clone(),id).await?;
    audit(db, c.user_id, id, "verification.decided", params).await?;
    Ok(result)
}
