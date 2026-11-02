"""Check bounded PDF attachment hypotheses through an owned synthetic MCP host.

Run only against the dedicated test server started by test_inspection_workflow.ps1.
All PDFs and application state are synthetic; no physical measurement is claimed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from generate_vector_drawing_fixtures import Drawing
from smoke_inspection_mcp import Mcp, fixtures


def attachment_traces(candidate):
    return [json.loads(value.split(':', 1)[1]) for value in candidate['evidence']
            if value.startswith('endpoint_attachment_hypothesis:')]


def run(folder, port):
    fixtures(folder)
    client = Mcp(folder, port)
    client.call('open_project', {'path': str(folder / 'fixture.dperproj')})
    results = []
    cases = ['solid', 'reversed', 'dashed', 'disconnected', 'outside_scope', 'concentric_conflict']
    for case in cases:
        drawing = Drawing()
        drawing.text(15, 20, 'SYNTHETIC ATTACHMENT HYPOTHESES', 9)
        drawing.circle(65, 65, 20)
        drawing.circle(225, 160, 20)
        if case == 'concentric_conflict':
            drawing.circle(65, 65, 30)
        points = [(158, 236), (125, 236), (125, 100), (85, 65)]
        if case == 'reversed':
            points.reverse()
        elif case == 'disconnected':
            points[0] = (120, 236)
        if case == 'dashed':
            drawing.command('[4 3] 0 d')
        drawing.polyline(points)
        drawing.command('[] 0 d')
        if case == 'concentric_conflict':
            drawing.polyline([(158, 236), (140, 220), (140, 95), (95, 65)])
        drawing.text(160, 240, 'Ø4.000 ±0.100 mm', 11)
        source = folder / f'{case}.pdf'
        source.write_bytes(drawing.pdf_bytes())
        before_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        context = client.call('get_measurement_context')
        state = client.call('analyze_drawing', {'path': str(source), 'expected_revision': context['document_revision']})
        features = [feature for feature in state['inspection']['plan']['characteristics']
                    if feature['kind'] == 'diameter' and feature['unit'] == 'millimeter']
        assert len(features) == 1, features
        feature = features[0]
        assert feature['nominal'] == 4 and not feature['confirmed']
        catalog = client.call('analyze_drawing_geometry', {'page': 1, 'expected_revision': state['document_revision']})
        listing = client.call('get_drawing_geometries', {'page': 1, 'limit': 100})
        circles = [primitive for primitive in listing['items'] if primitive['kind'] == 'circle_candidate']
        target = next(primitive for primitive in circles if abs(primitive['circle']['center']['x']-65) < .1
                      and abs(primitive['circle']['radius_pt']-20) < .1)
        assert len(circles) == (3 if case == 'concentric_conflict' else 2)
        geometry_ids = [primitive['id'] for primitive in listing['items']
                        if case != 'outside_scope' or primitive['kind'] == 'circle_candidate']
        defined = client.call('define_drawing_view', {'page': 1, 'expected_revision': catalog['document_revision'],
            'label': f'Explicit synthetic scope {case}', 'feature_ids': [feature['id']], 'geometry_ids': geometry_ids})
        arguments = {'feature_id': feature['id'], 'view_id': defined['view']['id']}
        candidates = client.call('get_drawing_binding_candidates', arguments)
        choices = candidates['candidates']
        assert len(choices) == len(circles), choices
        assert all('alternative_geometries_are_retained' in choice['suspension_reasons'] for choice in choices)
        assert all('binding_requires_explicit_review' in choice['suspension_reasons'] for choice in choices)
        target_choice = next(choice for choice in choices if choice['geometry_id'] == target['id'])
        traces = attachment_traces(target_choice)
        if case in ('solid', 'reversed'):
            assert traces and choices[0]['geometry_id'] == target['id'], choices
            assert all(trace['unique_within_selected_view'] and trace['requires_review'] for trace in traces)
            assert target_choice['distance_pt'] > min(choice['distance_pt'] for choice in choices), 'No nearer distractor'
        elif case == 'concentric_conflict':
            assert traces, choices
            assert all(not trace['unique_within_selected_view'] for trace in traces)
            assert choices[0]['geometry_id'] != target['id'], 'Conflicting attachments must not override proximity'
        else:
            assert not any(attachment_traces(choice) for choice in choices), choices
            assert choices[0]['geometry_id'] != target['id'], choices
        # A read is deterministic and never grants review, selection, or a native measurement.
        repeated = client.call('get_drawing_binding_candidates', arguments)
        assert candidates == repeated
        state = client.call('get_inspection_state')
        assert not state['inspection']['bindings'] and not state['inspection']['records']
        selected = client.call('select_drawing_binding', {'expected_revision': state['document_revision'],
            **arguments, 'candidate_id': target_choice['id'],
            'justification': 'Synthetic hypothesis only; source remains subject to explicit operator review.'})
        assert not selected['binding']['reviewed']
        assert hashlib.sha256(source.read_bytes()).hexdigest() == before_hash
        results.append({'case': case, 'passed': True, 'source_sha256': before_hash,
                        'feature_source': feature['source'], 'candidates': choices,
                        'selection_reviewed': selected['binding']['reviewed']})
    summary = {'passed': True, 'cases': results, 'physical_qualification': 'not_performed',
               'scope': 'Single open straight solid unfilled paths, endpoint hypotheses in explicit view; no arrow or general leader semantics'}
    (folder / 'attachment_summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps({'passed': True, 'case_count': len(results)}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--port', type=int, default=18766)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    destination = args.folder.resolve()
    if destination == root or not destination.is_relative_to(root) or destination.exists():
        parser.error('Use a new child folder of this DPER_MEASURE checkout to preserve existing evidence.')
    if not 18765 <= args.port <= 65535:
        parser.error('Use an owned dedicated test port, never the live user server.')
    run(destination, args.port)
