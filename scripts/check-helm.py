"""Validate chart ownership and the coordinated Argo CD deployment, without a cluster."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess

import yaml

ROOT = Path(__file__).resolve().parents[1]
CHARTS = ROOT / 'deploy/helm'
BUSINESS = ('exchange-api', 'exchange-worker', 'exchange')
INFRASTRUCTURE = ('exchange-postgres', 'exchange-monitoring')


def run(*args, success=True):
    result = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
    if (result.returncode == 0) != success:
        raise RuntimeError(f"Unexpected result for {' '.join(map(str, args))}:\n{result.stderr}\n{result.stdout}")
    return result.stdout


def identity(doc):
    return doc['apiVersion'], doc['kind'], doc['metadata']['name']


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--helm', default=shutil.which('helm') or 'helm')
    parser.add_argument('--kubeconform', default=shutil.which('kubeconform'))
    args = parser.parse_args()

    def validate(output):
        docs = [d for d in yaml.safe_load_all(output) if d]
        if args.kubeconform:
            core = [d for d in docs if not d['apiVersion'].startswith('external-secrets.io/')]
            result = subprocess.run([args.kubeconform, '-strict', '-summary', '-kubernetes-version', '1.27.0'],
                                    input=yaml.safe_dump_all(core), text=True, capture_output=True)
            if result.returncode:
                raise RuntimeError(result.stdout + result.stderr)
        return docs

    def render(chart, profile=None, overrides=(), release='dummy-exchange-dev'):
        location = CHARTS / chart
        options = ['-f', str(location / f'values-{profile}.yaml')] if profile else []
        options += [part for setting in overrides for part in ('--set', setting)]
        run(args.helm, 'lint', str(location), *options)
        docs = validate(run(args.helm, 'template', release, str(location), '--namespace', 'dummy-exchange', *options))
        for doc in docs:
            if doc['kind'] in ('Deployment', 'StatefulSet', 'DaemonSet', 'Job'):
                for container in doc['spec']['template']['spec']['containers']:
                    resources = container.get('resources', {})
                    assert all(resources.get(k, {}).get(r) for k in ('requests', 'limits') for r in ('cpu', 'memory')), identity(doc)
        return docs

    expected = {
        'exchange-api': {'api', 'migration'}, 'exchange-worker': {'worker'}, 'exchange': {'simulator', 'trader'},
        'exchange-postgres': {'postgres', 'redis'},
        'exchange-monitoring': {'prometheus', 'grafana', 'postgres-exporter', 'redis-exporter',
                                'kube-state-metrics', 'node-exporter', 'loki', 'alloy', 'tempo'},
    }
    actual = {p.name for p in CHARTS.iterdir() if (p / 'Chart.yaml').exists()}
    assert actual == set(expected) | {'exchange-tunnel'}, actual
    for profile in (None, 'dev', 'minikube'):
        release = 'dummy-exchange' if profile == 'minikube' else 'dummy-exchange-dev'
        prefix = 'exchange' if profile == 'minikube' else f'{release}-dummy-exchange'
        owned, workloads = {}, {}
        for chart in BUSINESS + INFRASTRUCTURE:
            assert not yaml.safe_load((CHARTS / chart / 'Chart.yaml').read_text()).get('dependencies')
            components = set()
            for doc in render(chart, profile, release=release):
                key = identity(doc)
                assert key not in owned, f'Duplicate ownership: {key}'
                owned[key] = chart
                if doc['kind'] in ('Deployment', 'StatefulSet', 'DaemonSet', 'Job'):
                    component = doc['metadata']['labels']['app.kubernetes.io/component']
                    components.add(component)
                    workloads[component] = doc
                    if doc['kind'] != 'Job':
                        assert doc['spec']['selector']['matchLabels'] == {
                            'app.kubernetes.io/name': 'dummy-exchange', 'app.kubernetes.io/instance': release,
                            'app.kubernetes.io/component': component,
                        }
                        assert doc['metadata']['name'] == f'{prefix}-{component}'
                    if chart in BUSINESS:
                        env = doc['spec']['template']['spec']['containers'][0]['env']
                        assert next(e['value'] for e in env if e['name'] == 'PGHOST') == f'{prefix}-postgres'
                        assert not any(e['name'] in ('DATABASE_URL', 'REDIS_URL') for e in env)
                if doc['kind'] == 'Service':
                    assert doc['spec']['selector']['app.kubernetes.io/instance'] == release
                if doc['kind'] == 'ExternalSecret':
                    assert len(doc['spec']['data']) == 1, 'Each owner synchronizes only its credential'
                    assert doc['metadata']['annotations']['argocd.argoproj.io/sync-wave'] == '-3'
                assert doc['kind'] != 'Ingress', 'Public traffic uses direct tunnel routes'
            trader_enabled = True
            if chart == "exchange" and profile:
                trader_enabled = yaml.safe_load((CHARTS/chart/f"values-{profile}.yaml").read_text()).get("traders",{}).get("enabled",True)
            expected_components = expected[chart] - ({"trader"} if chart == "exchange" and not trader_enabled else set())
            assert components == expected_components, (chart, components)
        for component in ('simulator', 'prometheus'):
            assert workloads[component]['spec']['strategy']['type'] == 'Recreate'
        assert workloads['simulator']['spec']['replicas'] == 1
        assert workloads['worker']['spec']['replicas'] == 2
        if 'trader' in workloads:
            assert workloads['trader']['metadata']['annotations']['argocd.argoproj.io/sync-wave'] == '1'
            assert workloads['trader']['spec']['podManagementPolicy'] == 'Parallel'
            assert not workloads['trader']['spec'].get('volumeClaimTemplates')
        job = workloads['migration']
        assert job['metadata']['name'] == f'{prefix}-migrate'
        assert job['metadata']['annotations']['argocd.argoproj.io/hook'] == 'Sync'
        assert job['metadata']['annotations']['argocd.argoproj.io/sync-wave'] == '-1'
        assert job['spec']['template']['spec']['containers'][0]['image'] == workloads['api']['spec']['template']['spec']['containers'][0]['image']
        for component in ('postgres', 'redis'):
            db = workloads[component]
            assert db['spec']['serviceName'] == f'{prefix}-{component}'
            assert db['spec']['volumeClaimTemplates'][0]['metadata']['name'] == 'data'
            assert db['metadata']['annotations']['argocd.argoproj.io/sync-options'] == 'Prune=false'
        if profile == 'dev':
            for component in ('api', 'worker', 'simulator'):
                assert '@sha256:' in workloads[component]['spec']['template']['spec']['containers'][0]['image']

    for profile in ('dev', 'minikube'):
        app = yaml.safe_load((ROOT / f'deploy/argocd/dummy-exchange-{profile}.yaml').read_text())
        assert 'source' not in app['spec']
        sources = app['spec']['sources']
        assert [s['path'].split('/')[-1] for s in sources] == list(BUSINESS)
        release = 'dummy-exchange-dev' if profile == 'dev' else 'dummy-exchange'
        assert {s['helm']['releaseName'] for s in sources} == {release}
        for source in sources:
            for filename in source['helm']['valueFiles']:
                assert (ROOT / source['path'] / filename).exists()
        for filename, chart in (('postgres', 'exchange-postgres'), ('monitoring', 'exchange-monitoring')):
            infra = yaml.safe_load((ROOT / f'deploy/argocd/{filename}-{profile}.yaml').read_text())
            assert infra['spec']['source']['path'] == f'deploy/helm/{chart}'
            assert infra['spec']['source']['helm']['releaseName'] == release
            assert not infra['spec']['syncPolicy']['automated']['prune']
            assert not infra['metadata'].get('finalizers')

    for chart in BUSINESS:
        for doc in render(chart, overrides=('database.host=external-db', 'database.port=5433', 'database.sslMode=require')):
            if doc['kind'] in ('Deployment', 'Job'):
                env = doc['spec']['template']['spec']['containers'][0]['env']
                assert next(e['value'] for e in env if e['name'] == 'PGHOST') == 'external-db'
                assert next(e['value'] for e in env if e['name'] == 'PGPORT') == '5433'
    assert not render('exchange-postgres', overrides=('enabled=false', 'redis.enabled=false'))
    assert not render('exchange-monitoring', overrides=('enabled=false',))
    monitoring = render('exchange-monitoring', overrides=('postgresExporter.enabled=false', 'redisExporter.enabled=false'))
    assert not any(d['metadata']['name'].endswith(('-postgres-exporter', '-redis-exporter')) for d in monitoring)
    config = next(d for d in monitoring if d['kind'] == 'ConfigMap' and 'prometheus.yml' in d['data'])
    assert not any(j['job_name'] in ('postgres', 'redis') for j in yaml.safe_load(config['data']['prometheus.yml'])['scrape_configs'])
    assert not any(a['alert'] in ('PostgreSQLUnavailable', 'RedisUnavailable') for a in yaml.safe_load(config['data']['alerts.yml'])['groups'][0]['rules'])
    dashboard = next(d for d in monitoring if d['kind'] == 'ConfigMap' and 'exchange-overview.json' in d['data'])
    assert json.loads(dashboard['data']['exchange-overview.json'])['panels']
    for filename in ('kubernetes-overview.json', 'kubernetes-logs.json'):
        assert json.loads(dashboard['data'][filename])['panels']
    grafana = next(d for d in monitoring if d['kind'] == 'Deployment' and d['metadata']['name'].endswith('-grafana'))
    assert grafana['spec']['strategy']['type'] == 'Recreate', 'Single-writer Grafana SQLite storage'
    assert grafana['spec']['template']['spec']['containers'][0]['startupProbe']
    for d in monitoring:
        if d['kind'] == 'ClusterRole':
            assert not any('secrets' in r.get('resources', []) for r in d['rules']), identity(d)
    minimal = render('exchange-monitoring', overrides=('kubernetes.enabled=false', 'logging.enabled=false', 'grafana.persistence.enabled=false'))
    assert not any(d['kind'] == 'ClusterRole' for d in minimal)
    assert not any(d['metadata']['name'].endswith(('-loki', '-alloy', '-kube-state-metrics', '-node-exporter')) for d in minimal)
    minimal_config = next(d for d in minimal if d['kind'] == 'ConfigMap' and 'prometheus.yml' in d['data'])
    assert not any(j['job_name'] in ('cadvisor', 'kubelet', 'loki', 'alloy', 'node-exporter', 'kube-state-metrics') for j in yaml.safe_load(minimal_config['data']['prometheus.yml'])['scrape_configs'])
    bridge = render('exchange-postgres', 'dev', overrides=('externalSecrets.legacyKeys.jwt-secret=old-jwt', 'externalSecrets.legacyKeys.grafana-admin-password=old-grafana'))
    assert len(next(d for d in bridge if d['kind'] == 'ExternalSecret')['spec']['data']) == 3
    for chart, setting in [('exchange-api', 'replicas=0'), ('exchange-worker', 'replicas=0'),
                           ('exchange', 'database.port=0'),
                           ('exchange-api', 'externalSecrets.enabled=true')]:
        run(args.helm, 'template', 'test', str(CHARTS / chart), '--set', setting, success=False)
    render('exchange-tunnel')
    render('exchange-tunnel', overrides=('enabled=true',))
    tunnel_app = yaml.safe_load((ROOT / 'deploy/argocd/tunnel-dev.yaml').read_text())
    tunnel_values = tunnel_app['spec']['source']['helm']['valuesObject']
    assert tunnel_values['enabled'] and tunnel_values['replicas'] == 2
    assert tunnel_app['spec']['destination']['namespace'] == 'ingress'
    print('Five-chart profiles, ownership, migration ordering, secrets, storage and direct tunnel checks passed.')
    if not args.kubeconform:
        print('Kubernetes schema validation skipped: kubeconform is not installed.')


if __name__ == '__main__':
    main()
