//! Analytic image oracles: no production profile positions, gradients or fit
//! routines are used to rasterize these scenes or define their true contours.
use super::*;
use image::Rgb;

fn raster(intensity: impl Fn(f64, f64) -> f64) -> RgbImage {
    RgbImage::from_fn(256, 256, |x, y| {
        let value = intensity(x as f64, y as f64).round().clamp(0.0, 255.0) as u8;
        Rgb([value; 3])
    })
}

fn transition(distance: f64) -> f64 {
    (1.0 + (distance / 0.65).tanh()) * 0.5
}

fn line(center: Point, angle: f64) -> InspectionGeometry {
    InspectionGeometry::Line {
        start: Point {
            x: center.x - 80.0 * angle.cos(),
            y: center.y - 80.0 * angle.sin(),
        },
        end: Point {
            x: center.x + 80.0 * angle.cos(),
            y: center.y + 80.0 * angle.sin(),
        },
    }
}

fn assert_ambiguous(image: &RgbImage, geometry: &InspectionGeometry) {
    let error = inspect_geometry(image, geometry, &VisionOptions::default()).unwrap_err();
    assert!(error.contains("ambigu"), "{geometry:?}: {error}");
    assert!(error.contains("requis"), "{error}");
}

fn assert_line_offset(result: &InspectionResult, center: Point, angle: f64, truth: f64) {
    let InspectionGeometry::Line { start, end } = result.geometry else {
        panic!("line expected")
    };
    for point in [start, end] {
        let offset = -(point.x - center.x) * angle.sin() + (point.y - center.y) * angle.cos();
        assert!(
            (offset - truth).abs() < 0.2,
            "offset {offset}, truth {truth}"
        );
    }
    assert!(!result.evidence.endpoint_extent_verified);
}

fn assert_radius(result: &InspectionResult, truth: f64) {
    let radius = match result.geometry {
        InspectionGeometry::Circle { radius, .. } | InspectionGeometry::Arc { radius, .. } => {
            radius
        }
        _ => panic!("radial geometry expected"),
    };
    assert!(
        (radius - truth).abs() < 0.25,
        "radius {radius}, truth {truth}"
    );
}

#[test]
fn symmetric_double_lines_refuse_both_equal_and_unequal_strengths_at_multiple_angles() {
    let center = Point { x: 125.3, y: 129.7 };
    for angle in [0.0_f64, 0.43, 1.7, 2.8] {
        for strengths in [(100.0, 100.0), (60.0, 150.0)] {
            let image = raster(|x, y| {
                let distance = -(x - center.x) * angle.sin() + (y - center.y) * angle.cos();
                20.0 + strengths.0 * transition(distance + 3.0)
                    + strengths.1 * transition(distance - 3.0)
            });
            assert_ambiguous(&image, &line(center, angle));
        }
    }
}

#[test]
fn symmetric_double_circles_and_arcs_refuse_across_radii_and_sweep_directions() {
    let center = Point { x: 127.3, y: 128.6 };
    for radius in [18.0, 48.0, 86.0] {
        let image = raster(|x, y| {
            let distance = (x - center.x).hypot(y - center.y) - radius;
            20.0 + 100.0 * transition(distance + 3.0) + 100.0 * transition(distance - 3.0)
        });
        assert_ambiguous(&image, &InspectionGeometry::Circle { center, radius });
        for sweep_rad in [2.5, -2.5] {
            assert_ambiguous(
                &image,
                &InspectionGeometry::Arc {
                    center,
                    radius,
                    start_rad: 0.37,
                    sweep_rad,
                },
            );
        }
    }
}

#[test]
fn polarity_resolves_opposite_edges_without_using_the_nominal_dimension() {
    let center = Point { x: 127.3, y: 128.6 };
    let angle = 0.61_f64;
    let line_image = raster(|x, y| {
        let distance = -(x - center.x) * angle.sin() + (y - center.y) * angle.cos();
        20.0 + 210.0 * (transition(distance + 3.0) - transition(distance - 3.0))
    });
    assert_ambiguous(&line_image, &line(center, angle));
    let radius = 54.7;
    let radial_image = raster(|x, y| {
        let distance = (x - center.x).hypot(y - center.y) - radius;
        20.0 + 210.0 * (transition(distance + 3.0) - transition(distance - 3.0))
    });
    for (polarity, truth_offset) in [(EdgePolarity::Rising, -3.0), (EdgePolarity::Falling, 3.0)] {
        let options = VisionOptions {
            polarity,
            ..VisionOptions::default()
        };
        let result = inspect_geometry(&line_image, &line(center, angle), &options).unwrap();
        assert_line_offset(&result, center, angle, truth_offset);
        for geometry in [
            InspectionGeometry::Circle { center, radius },
            InspectionGeometry::Arc {
                center,
                radius,
                start_rad: 0.4,
                sweep_rad: -2.6,
            },
        ] {
            let result = inspect_geometry(&radial_image, &geometry, &options).unwrap();
            assert_radius(&result, radius + truth_offset);
        }
    }
}

#[test]
fn a_clear_localization_prior_resolves_same_polarity_neighbors() {
    let center = Point { x: 127.3, y: 128.6 };
    let image = raster(|x, _| 20.0 + 60.0 * transition(x - 120.3) + 150.0 * transition(x - 126.3));
    let geometry = InspectionGeometry::Line {
        start: Point { x: 121.8, y: 30.0 },
        end: Point { x: 121.8, y: 225.0 },
    };
    let result = inspect_geometry(&image, &geometry, &VisionOptions::default()).unwrap();
    let InspectionGeometry::Line { start, end } = result.geometry else {
        panic!()
    };
    assert!((start.x - 120.3).abs() < 0.2);
    assert!((end.x - 120.3).abs() < 0.2);
    let radius = 52.0;
    let image = raster(|x, y| {
        let distance = (x - center.x).hypot(y - center.y) - radius;
        20.0 + 60.0 * transition(distance + 3.0) + 150.0 * transition(distance - 3.0)
    });
    for geometry in [
        InspectionGeometry::Circle {
            center,
            radius: radius - 1.5,
        },
        InspectionGeometry::Arc {
            center,
            radius: radius - 1.5,
            start_rad: 2.1,
            sweep_rad: 2.7,
        },
    ] {
        let result = inspect_geometry(&image, &geometry, &VisionOptions::default()).unwrap();
        assert_radius(&result, radius - 3.0);
    }
}

#[test]
fn subpixel_prior_differences_do_not_arbitrarily_resolve_double_edges() {
    let image = raster(|_, y| 20.0 + 100.0 * transition(y - 121.3) + 100.0 * transition(y - 127.3));
    for prior_y in [124.0, 124.3, 124.6] {
        assert_ambiguous(
            &image,
            &line(
                Point {
                    x: 127.0,
                    y: prior_y,
                },
                0.0,
            ),
        );
    }
}

#[test]
fn localized_reflection_profiles_are_excluded_but_generalized_reflections_refuse() {
    let geometry = line(Point { x: 127.0, y: 124.3 }, 0.0);
    for generalized in [false, true] {
        let image = raster(|x, y| {
            if generalized || (109.0..143.0).contains(&x) {
                // A clipped bright reflected band has two plausible contours.
                30.0 + 400.0 * (transition(y - 121.3) - transition(y - 127.3))
            } else {
                25.0 + 205.0 * transition(y - 124.3)
            }
        });
        if generalized {
            assert_ambiguous(&image, &geometry);
        } else {
            let result = inspect_geometry(&image, &geometry, &VisionOptions::default()).unwrap();
            assert_line_offset(&result, Point { x: 127.0, y: 124.3 }, 0.0, 0.0);
            assert!(result.evidence.coverage_fraction > 0.7);
            assert!(result.evidence.coverage_fraction < 0.9);
            assert!(!result.evidence.rejected_points.is_empty());
        }
    }
}

#[test]
fn localized_saturation_can_leave_support_but_generalized_saturation_cannot() {
    let center = Point { x: 127.0, y: 124.3 };
    let geometry = line(center, 0.0);
    for generalized in [false, true] {
        let image = raster(|x, y| {
            if generalized || (109.0..143.0).contains(&x) {
                255.0
            } else {
                25.0 + 205.0 * transition(y - center.y)
            }
        });
        if generalized {
            let error = inspect_geometry(&image, &geometry, &VisionOptions::default()).unwrap_err();
            assert!(error.contains("insuffisant"), "{error}");
        } else {
            let result = inspect_geometry(&image, &geometry, &VisionOptions::default()).unwrap();
            assert_line_offset(&result, center, 0.0, 0.0);
            assert!(result.evidence.coverage_fraction > 0.7);
            assert!(result.evidence.coverage_fraction < 0.9);
        }
    }
}

#[test]
fn one_raster_peak_is_not_mistaken_for_two_edges_on_a_plateau() {
    // Integer and half-integer transitions generate symmetric adjacent gradient
    // samples. They must remain a single contour after peak grouping.
    for truth in [123.0, 123.5] {
        let image = raster(|_, y| 20.0 + 210.0 * transition(y - truth));
        let center = Point {
            x: 127.0,
            y: truth + 0.5,
        };
        let result =
            inspect_geometry(&image, &line(center, 0.0), &VisionOptions::default()).unwrap();
        assert_line_offset(&result, center, 0.0, -0.5);
        assert!(result.evidence.coverage_fraction > 0.95);
    }
}

#[test]
fn small_and_large_single_circles_recover_analytic_centers_and_radii_under_noise() {
    for (center, radius) in [
        (
            Point {
                x: 104.35,
                y: 113.7,
            },
            14.7,
        ),
        (Point { x: 127.6, y: 126.1 }, 93.2),
    ] {
        let image = raster(|x, y| {
            let distance = (x - center.x).hypot(y - center.y) - radius;
            25.0 + 205.0 * transition(distance) + 1.2 * (0.58 * x + 1.27 * y).sin()
        });
        let localization = InspectionGeometry::Circle {
            center: Point {
                x: center.x + 1.3,
                y: center.y - 0.8,
            },
            radius: radius + 2.1,
        };
        let result = inspect_geometry(&image, &localization, &VisionOptions::default()).unwrap();
        let InspectionGeometry::Circle {
            center: actual,
            radius: measured,
        } = result.geometry
        else {
            panic!("circle expected")
        };
        assert!(
            actual.dist(center) < 0.18,
            "center {actual:?}, truth {center:?}"
        );
        assert!(
            (measured - radius).abs() < 0.18,
            "radius {measured}, truth {radius}"
        );
        assert!(result.evidence.coverage_fraction > 0.95);
        assert!(result.evidence.rms_residual_px < 0.2);
    }
}

#[test]
fn angles_from_two_independent_line_supports_match_analytic_angles_under_rotation_and_noise() {
    // Both scenes use the same pixel coordinate frame. Their independently
    // fitted support directions determine this test-only angle. No application
    // angle feature, intersection, segment length or physical endpoint detector
    // is being exercised; the observed spans never verify part endpoints.
    for truth_degrees in [30.0_f64, 60.0, 90.0, 135.0] {
        let truth_angle = truth_degrees.to_radians();
        for rotation in [0.13_f64, 0.83, 2.34] {
            let mut directions = Vec::new();
            for (center, angle) in [
                (Point { x: 117.3, y: 111.7 }, rotation),
                (Point { x: 137.6, y: 141.2 }, rotation + truth_angle),
            ] {
                let image = raster(|x, y| {
                    let distance = -(x - center.x) * angle.sin() + (y - center.y) * angle.cos();
                    25.0 + 205.0 * transition(distance) + 1.5 * (0.63 * x + 1.19 * y).sin()
                });
                let localization_center = Point {
                    x: center.x - 1.2 * angle.sin(),
                    y: center.y + 1.2 * angle.cos(),
                };
                let result = inspect_geometry(
                    &image,
                    &line(localization_center, angle),
                    &VisionOptions::default(),
                )
                .unwrap();
                assert_line_offset(&result, center, angle, 0.0);
                let InspectionGeometry::Line { start, end } = result.geometry else {
                    panic!("line expected")
                };
                let length = start.dist(end);
                directions.push(Point {
                    x: (end.x - start.x) / length,
                    y: (end.y - start.y) / length,
                });
            }
            let dot = directions[0].x * directions[1].x + directions[0].y * directions[1].y;
            let measured_angle = dot.clamp(-1.0, 1.0).acos();
            assert!(
                (measured_angle - truth_angle).abs().to_degrees() < 0.2,
                "angle {} deg, truth {truth_degrees} deg, global rotation {rotation} rad",
                measured_angle.to_degrees()
            );
        }
    }
}
