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
            if value.startswith(('endpoint_attachment_hypothesis:', 'endpoint_attachment_chain_hypothesis:'))]


def run(folder, port):
    fixtures(folder)
    client = Mcp(folder, port)
    client.call('open_project', {'path': str(folder / 'fixture.dperproj')})
    results = []
    cases = ['solid', 'reversed', 'dashed', 'disconnected', 'outside_scope', 'concentric_conflict',
             'split', 'split_reversed', 'split_small_gap', 'split_large_gap', 'split_branch',
             'split_t_contact', 'split_crossing', 'split_near_t_contact', 'split_near_crossing',
             'split_cycle', 'split_outside_scope', 'split_with_disconnected_branch', 'solid_with_disconnected_cycle']
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
        if case.startswith('split'):
            paths = [[(158, 236), (125, 236)], [(125, 236), (125, 100)], [(125, 100), (85, 65)]]
            if case in ('split_small_gap', 'split_large_gap'):
                paths[1][0] = (125, 236 - (0.2 if case == 'split_small_gap' else 0.4))
            elif case in ('split_near_t_contact', 'split_near_crossing'):
                paths[1] = [(125.2, 236 if case == 'split_near_t_contact' else 236.1), (125.2, 100)]
            if case == 'split_reversed':
                paths = [list(reversed(path)) for path in reversed(paths)]
            for path in paths:
                drawing.polyline(path)
            if case == 'split_branch':
                drawing.polyline([(125, 236), (150, 210)])
            elif case == 'split_t_contact':
                drawing.polyline([(125, 160), (150, 160)])
            elif case == 'split_crossing':
                drawing.polyline([(110, 160), (140, 160)])
            elif case == 'split_cycle':
                drawing.polyline([(125, 100), (150, 160), (125, 236)])
        else:
            drawing.polyline(points)
        drawing.command('[] 0 d')
        if case == 'concentric_conflict':
            drawing.polyline([(158, 236), (140, 220), (140, 95), (95, 65)])
        if case == 'split_with_disconnected_branch':
            for branch in [[(300, 40), (320, 40)], [(320, 40), (330, 50)], [(320, 40), (330, 30)]]:
                drawing.polyline(branch)
        elif case == 'solid_with_disconnected_cycle':
            for edge in [[(300, 40), (330, 40)], [(330, 40), (330, 70)], [(330, 70), (300, 40)]]:
                drawing.polyline(edge)
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
        def include_in_scope(primitive):
            if case == 'outside_scope':
                return primitive['kind'] == 'circle_candidate'
            if case == 'split_outside_scope':
                box = primitive['bbox']
                return not (abs(box['x_min'] - 125) < .1 and abs(box['x_max'] - 125) < .1
                            and box['y_max'] - box['y_min'] > 100)
            return True
        geometry_ids = [primitive['id'] for primitive in listing['items'] if include_in_scope(primitive)]
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
        if case in ('solid', 'reversed', 'split', 'split_reversed', 'split_small_gap',
                    'split_with_disconnected_branch', 'solid_with_disconnected_cycle'):
            assert traces and choices[0]['geometry_id'] == target['id'], choices
            assert all(trace['unique_within_selected_view'] and trace['requires_review'] for trace in traces)
            assert target_choice['distance_pt'] > min(choice['distance_pt'] for choice in choices), 'No nearer distractor'
            if case.startswith('split'):
                assert any(line.startswith('endpoint_attachment_chain_hypothesis:') for line in target_choice['evidence'])
                chain, = traces
                assert len(chain['ordered_supports']) == 3 and len(chain['joints']) == 2, chain
                assert chain['joint_tolerance_pt'] == 0.25 and chain['tolerance_pt'] == 2, chain
                for joint in chain['joints']:
                    assert 0 <= joint['gap_pt'] <= chain['joint_tolerance_pt'], joint
                    dx = joint['from_endpoint']['x'] - joint['to_endpoint']['x']
                    dy = joint['from_endpoint']['y'] - joint['to_endpoint']['y']
                    assert abs(joint['gap_pt'] - (dx*dx + dy*dy)**.5) < 1e-9
                assert all(support['source']['path_data'] and support['support_fingerprint']
                           for support in chain['ordered_supports'])
                if case == 'split_small_gap':
                    assert any(joint['gap_pt'] > 0.15 for joint in chain['joints']), 'The accepted source gap was snapped away'
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
               'scope': 'Single paths and bounded simple split-path chains in explicit view; no arrow or general leader semantics'}
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
