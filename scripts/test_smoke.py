#!/usr/bin/env python3
"""Read-only HTTP smoke checks. Start the backend first; defaults to localhost:3000."""
import gzip
import json
import os
import sys
import urllib.error
import urllib.request

BASE = os.environ.get('WORLDMAP_TEST_URL', 'http://127.0.0.1:3000')
failures = []

def get(path, headers=None):
    request = urllib.request.Request(BASE + path, headers=headers or {})
    try:
        with urllib.request.urlopen(request, timeout=40) as response:
            body = response.read()
            if response.headers.get('Content-Encoding') == 'gzip':
                body = gzip.decompress(body)
            return response.status, response.headers, body
    except urllib.error.HTTPError as error:
        return error.code, error.headers, error.read()

def check(name, fn):
    try:
        fn()
        print('PASS', name)
    except Exception as error:
        failures.append(name)
        print('FAIL', name, str(error))

def status_is(path, expected):
    status, _, _ = get(path)
    assert status == expected, f'{path}: expected {expected}, got {status}'

def dataset(path):
    status, headers, body = get(path, {'Accept-Encoding': 'gzip'})
    assert status == 200
    data = json.loads(body)
    assert data['type'] == 'FeatureCollection'
    assert isinstance(data['features'], list)
    assert headers.get('Content-Type').startswith('application/')
    print('    features:', len(data['features']))

def static_cache():
    status, headers, _ = get('/api/airports', {'Accept-Encoding': 'gzip'})
    assert status == 200
    assert 'max-age=3600' in headers.get('Cache-Control', '')
    assert headers.get('Content-Encoding') == 'gzip'

def history():
    status, _, body = get('/api/history/timestamps')
    assert status == 200
    timestamps = json.loads(body)['timestamps']
    if timestamps:
        ts = max(timestamps)
        status, _, body = get(f'/api/history/ships?from={ts}&to={ts}')
        assert status == 200 and isinstance(json.loads(body), list)
    print('    snapshots:', len(timestamps))

for path in ('airports', 'seaports', 'reactors', 'ships/snapshot', 'ships/aton'):
    check(path, lambda p=path: dataset('/api/' + p))
for path in ('/api/status', '/api/alerts/count', '/api/watchlist', '/api/events', '/api/export/report'):
    check(path, lambda p=path: status_is(p, 200))
check('static JSON caching and gzip', static_cache)
check('historical timestamps and snapshot', history)
check('invalid latitude rejected', lambda: status_is('/api/weather?lat=91&lon=0', 400))
check('invalid longitude rejected', lambda: status_is('/api/weather?lat=0&lon=181', 400))
check('short search rejected', lambda: status_is('/api/search?q=x', 400))
check('unknown tile source', lambda: status_is('/tiles/missing/2/1/1', 404))
check('unknown export type', lambda: status_is('/api/export/csv?type=missing', 400))
print(f'\n{17 - len(failures)}/17 checks passed')
sys.exit(bool(failures))
