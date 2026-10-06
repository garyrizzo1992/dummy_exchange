"""Wait for Terraform to create Pages before CI uploads the frontend."""
import json
import os
import time
from urllib.error import HTTPError
from urllib.request import Request, urlopen

account = os.environ['CLOUDFLARE_ACCOUNT_ID']
project = os.environ['PAGES_PROJECT']
token = os.environ['CLOUDFLARE_API_TOKEN']
url = f'https://api.cloudflare.com/client/v4/accounts/{account}/pages/projects/{project}'
for attempt in range(60):
    try:
        with urlopen(Request(url, headers={'Authorization': f'Bearer {token}'}), timeout=20) as response:
            if json.load(response)['success']:
                print(f'Pages project ready: {project}')
                break
    except HTTPError as error:
        if error.code not in (404, 429, 500, 502, 503, 504):
            raise SystemExit(f'Pages API returned HTTP {error.code}; check the token and its Pages Edit permission') from None
    if attempt == 59:
        raise SystemExit('Pages project was not ready after ten minutes; check the infrastructure workflow')
    time.sleep(10)
