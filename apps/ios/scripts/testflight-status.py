#!/usr/bin/env python3
"""Prints the newest iPhone builds in App Store Connect and whether Apple has
finished processing them, using the API key that make ios-upload uses.

    apps/ios/scripts/testflight-status.py
"""
import base64
import json
import pathlib
import time
import urllib.parse
import urllib.request

from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives.asymmetric.utils import decode_dss_signature

BUNDLE_ID = "com.borisnezlobin.gasp"
SETTINGS = pathlib.Path.home() / ".appstoreconnect/gasp.env"
API = "https://api.appstoreconnect.apple.com/v1"


def settings():
    pairs = (line.split("=", 1) for line in SETTINGS.read_text().splitlines() if "=" in line)
    return {key.strip(): value.strip() for key, value in pairs}


def b64(data):
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def token(key_id, issuer):
    key_path = pathlib.Path.home() / f".appstoreconnect/private_keys/AuthKey_{key_id}.p8"
    key = serialization.load_pem_private_key(key_path.read_bytes(), password=None)
    header = b64(json.dumps({"alg": "ES256", "kid": key_id, "typ": "JWT"}).encode())
    now = int(time.time())
    claims = {"iss": issuer, "iat": now, "exp": now + 600, "aud": "appstoreconnect-v1"}
    payload = b64(json.dumps(claims).encode())
    signature = key.sign(f"{header}.{payload}".encode(), ec.ECDSA(hashes.SHA256()))
    r, s = decode_dss_signature(signature)
    return f"{header}.{payload}.{b64(r.to_bytes(32, 'big') + s.to_bytes(32, 'big'))}"


def get(path, query, bearer):
    url = f"{API}/{path}?{urllib.parse.urlencode(query)}"
    request = urllib.request.Request(url, headers={"Authorization": f"Bearer {bearer}"})
    with urllib.request.urlopen(request) as response:
        return json.load(response)


def main():
    config = settings()
    bearer = token(config["ASC_KEY_ID"], config["ASC_ISSUER_ID"])
    apps = get("apps", {"filter[bundleId]": BUNDLE_ID}, bearer)["data"]
    if not apps:
        print(f"No app with bundle ID {BUNDLE_ID} in App Store Connect.")
        return
    builds = get(
        "builds",
        {"filter[app]": apps[0]["id"], "sort": "-uploadedDate", "limit": 5,
         "include": "preReleaseVersion", "fields[builds]": "version,processingState,uploadedDate,expired,preReleaseVersion"},
        bearer,
    )
    versions = {item["id"]: item["attributes"]["version"] for item in builds.get("included", [])}
    for build in builds["data"]:
        attributes = build["attributes"]
        release = versions.get(build["relationships"]["preReleaseVersion"]["data"]["id"], "?")
        print(f"{release} ({attributes['version']}): {attributes['processingState'].lower()}, uploaded {attributes['uploadedDate']}")


if __name__ == "__main__":
    main()
