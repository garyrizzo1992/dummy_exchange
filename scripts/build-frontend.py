"""Build a standalone static frontend; requires Rust and wasm-bindgen-cli 0.2.128."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--api-origin', default='', help='API origin, e.g. https://api.garyrizzo.dev; default: same origin')
    args = parser.parse_args()
    origin = args.api_origin.rstrip('/')
    if origin:
        parsed = urlsplit(origin)
        if parsed.scheme not in ('http', 'https') or not parsed.netloc or parsed.path or parsed.query or parsed.fragment or parsed.username or parsed.password:
            parser.error('--api-origin must be an HTTP(S) origin without a path or credentials')
    version = subprocess.check_output(['wasm-bindgen', '--version'], text=True).strip()
    if version != 'wasm-bindgen 0.2.128':
        parser.error('Install wasm-bindgen-cli 0.2.128 to match Cargo.lock')
    output = ROOT / 'target' / 'frontend-static'
    env = dict(os.environ, EXCHANGE_API_ORIGIN=origin)
    subprocess.run(['cargo', 'build', '--release', '--locked', '-p', 'exchange-frontend', '--target', 'wasm32-unknown-unknown', '--target-dir', str(ROOT / 'target')], cwd=ROOT, env=env, check=True)
    output.mkdir(parents=True, exist_ok=True)
    subprocess.run(['wasm-bindgen', '--target', 'web', '--out-name', 'exchange_frontend', '--out-dir', str(output / 'pkg'), str(ROOT / 'target/wasm32-unknown-unknown/release/exchange-frontend.wasm')], cwd=ROOT, check=True)
    frontend = ROOT / 'crates' / 'frontend'
    (output / 'index.html').write_text((frontend / 'index.html').read_text().replace('/v1/ui/', './'), encoding='utf-8')
    shutil.copyfile(frontend / 'style.css', output / 'style.css')
    shutil.copyfile(frontend / 'dist/pkg/bootstrap.js', output / 'pkg/bootstrap.js')
    connect_sources = "'self'" + (f' {origin}' if origin else '')
    (output / '_headers').write_text(
        '/*\n'
        '  Cache-Control: no-cache\n'
        '  X-Content-Type-Options: nosniff\n'
        '  X-Frame-Options: DENY\n'
        '  Referrer-Policy: no-referrer\n'
        "  Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; "
        f"style-src 'self'; connect-src {connect_sources}; img-src 'self' data:; "
        "frame-ancestors 'none'; base-uri 'none'; form-action 'self'\n",
        encoding='utf-8',
    )
    archive = shutil.make_archive(str(ROOT / 'target' / 'exchange-frontend-static'), 'zip', output)
    print(f'Static frontend: {output}')
    print(f'Archive: {archive}')


if __name__ == '__main__':
    main()
