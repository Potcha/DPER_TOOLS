// Included in exports::tests, reusing the independent export session fixture.
fn evidence_record_for_export(measurement: &Measurement) -> crate::dper_inspection::InspectionRecord {
    serde_json::from_value(serde_json::json!({
        "feature_id":"diameter-A", "group":"Groupe; témoin", "capture_id":measurement.capture_id,
        "source_fingerprint":42, "calibration_id":null,
        "result":{"geometry":{"Circle":{"center":{"x":10.0,"y":10.0},"radius":4.0}},
            "evidence":{"accepted_points":[{"x":14.0,"y":10.0}],"rejected_points":[],"sample_count":96,
                "rms_residual_px":0.125,"max_residual_px":0.25,"coverage_fraction":0.9,"endpoint_extent_verified":false}},
        "value_mm":0.8, "outcome":{"geometry_state":"validated","conformity_status":"indeterminate",
            "blocking_reasons":["line_endpoints_unverified"]}, "measurement_id":measurement.id,
        "measurement_points":measurement.points, "vision_options":crate::dper_vision::VisionOptions {
            maximum_rms_px:0.35, search_radius_px:6.0, ..Default::default() }
    })).unwrap()
}

#[test]
fn csv_xlsx_inspection_metadata_is_historical_source_joined_and_escaped() {
    let mut session = test_session(true);
    session.measurements[0].origin = "vision".into();
    session.measurements[0].source_measurement_ids = vec![7,8];
    let record = evidence_record_for_export(&session.measurements[0]);
    let mut foreign = record.clone();
    foreign.capture_id = "foreign-capture".into(); foreign.group = "WRONG_CAPTURE_GROUP".into();
    session.automated_inspection = Some(crate::dper_inspection::InspectionWorkspace {
        records:vec![record,foreign], ..Default::default()
    });
    let csv=String::from_utf8(measurements_csv(&session).unwrap()).unwrap();
    assert!(csv.lines().next().unwrap().ends_with("dependances;motifs_suspension"));
    assert!(csv.contains("\"Groupe; témoin\""));
    assert!(!csv.contains("WRONG_CAPTURE_GROUP"));
    for expected in ["historique : vérifiée par logiciel","Recherche 6.000 px","RMS max 0.350 px",
        "M7, M8","extrémités réelles non mesurées","conformité suspendue"] { assert!(csv.contains(expected),"{expected}"); }
    let xlsx=measurements_xlsx(&session).unwrap();
    let xml=String::from_utf8_lossy(&xlsx);
    for expected in ["Groupe inspection","Groupe; témoin","historique : vérifiée par logiciel",
        "RMS max 0.350 px","M7, M8","extrémités réelles non mesurées"] { assert!(xml.contains(expected),"{expected}"); }
}

#[test]
fn legacy_inspection_metadata_never_invents_thresholds_and_stale_values_stay_omitted() {
    let mut session=test_session(true);
    let mut record=evidence_record_for_export(&session.measurements[0]);
    let mut value=serde_json::to_value(&record).unwrap();
    value.as_object_mut().unwrap().remove("vision_options");
    record=serde_json::from_value(value).unwrap();
    assert!(record.vision_options.is_none());
    session.automated_inspection=Some(crate::dper_inspection::InspectionWorkspace { records:vec![record], ..Default::default() });
    let csv=String::from_utf8(measurements_csv(&session).unwrap()).unwrap();
    assert!(csv.contains("seuils non conservés"));
    session.measurements[0].dependency_stale=true;
    session.measurements[0].label="STALE_NUMERIC_VALUE".into();
    let csv=String::from_utf8(measurements_csv(&session).unwrap()).unwrap();
    assert_eq!(csv.lines().count(),1);
    let xlsx=measurements_xlsx(&session).unwrap();
    assert!(!String::from_utf8_lossy(&xlsx).contains("STALE_NUMERIC_VALUE"));
}
