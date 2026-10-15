import io
import json
import time
import unittest
import urllib.error
from unittest.mock import patch

from test_connector import HelperTestCase, FakeResponse, TOKEN, MINTED

CLIENT_ID = 'amzn1.application-oa2-client.TESTCLIENTID'
CLIENT_SECRET = 'amzn-secret-VALUE-1234567890'
REFRESH = 'Atzr|REFRESH-TOKEN-VALUE-abcdef'
HOST = 'sellingpartnerapi-na.amazon.com'
API = f'https://{HOST}/listings/2021-08-01/items/SELLER/SKU-1?marketplaceIds=ATVPDKIKX0DER'


class AmazonDirectApiTests(HelperTestCase):
    """The seller's own private SP-API app: client ID + secret + refresh token."""

    def link(self, token=TOKEN, expires_in=3600, host=HOST, refresh=REFRESH, client_id=CLIENT_ID, client_secret=CLIENT_SECRET):
        creds = {'api_host': host, 'region': 'na'}
        if client_id:
            creds['client_id'] = client_id
        if client_secret:
            creds['client_secret'] = client_secret
        if refresh:
            creds['refresh_token'] = refresh
        self.write('amazon_sp', {'auth_type': 'pat', 'access_token': token, 'credentials': creds,
                                 'expires_at': int(time.time()) + expires_in if token else 0})

    def opener(self, mint=None, api=None):
        calls = {'mint': [], 'api': []}

        def respond(handler, req):
            if isinstance(handler, BaseException):
                raise handler
            return handler(req) if callable(handler) else FakeResponse(handler)

        def fake_open(req, timeout):
            if req.full_url == 'https://api.amazon.com/auth/o2/token':
                calls['mint'].append(req)
                return respond(mint if mint is not None else json.dumps({'access_token': MINTED, 'token_type': 'bearer', 'expires_in': 3600}), req)
            calls['api'].append(req)
            return respond(api if api is not None else '{"sku": "SKU-1"}', req)

        return calls, fake_open

    def call(self, fake_open, url=API, method='GET', *extra):
        with patch.object(self.mod._OPENER, 'open', fake_open):
            return self.run_main('call', 'amazon_sp', method, url, *extra)

    def test_valid_stored_token_goes_in_the_amazon_header_without_minting(self):
        self.link()
        calls, fake = self.opener()
        code, out, _ = self.call(fake)
        self.assertEqual(code, 0, out)
        self.assertEqual(calls['mint'], [])
        req = calls['api'][0]
        self.assertEqual(req.get_header('X-amz-access-token'), TOKEN)
        self.assertIsNone(req.get_header('Authorization'))

    def test_expired_token_is_minted_with_the_refresh_token_from_login_with_amazon(self):
        self.link(expires_in=-10)
        calls, fake = self.opener()
        code, _, _ = self.call(fake)
        self.assertEqual(code, 0)
        self.assertEqual(len(calls['mint']), 1)
        body = dict(x.split('=', 1) for x in calls['mint'][0].data.decode().split('&'))
        self.assertEqual(body['grant_type'], 'refresh_token')
        for key in ('refresh_token', 'client_id', 'client_secret'):
            self.assertIn(key, body)
        self.assertEqual(calls['api'][0].get_header('X-amz-access-token'), MINTED)

    def test_without_a_refresh_token_an_expired_token_asks_to_reconnect(self):
        self.link(expires_in=-10, refresh='')
        calls, fake = self.opener()
        code, _, err = self.call(fake)
        self.assertNotEqual(code, 0)
        self.assertEqual(calls['mint'], [])
        self.assertIn('reconnect', err)

    def test_rejected_refresh_token_asks_to_reconnect_and_echoes_nothing(self):
        self.link(expires_in=-10)
        _, fake = self.opener(mint=urllib.error.HTTPError('u', 400, 'bad', {}, io.BytesIO(b'{"error":"invalid_grant"}')))
        code, out, err = self.call(fake)
        self.assertEqual(code, 3)
        for secret in (CLIENT_SECRET, REFRESH, CLIENT_ID):
            self.assertNotIn(secret, out + err)

    def test_rejected_token_is_reminted_once_and_retried(self):
        self.link()
        calls = {'api': 0, 'mint': 0}

        def fake_open(req, timeout):
            if req.full_url == 'https://api.amazon.com/auth/o2/token':
                calls['mint'] += 1
                return FakeResponse(json.dumps({'access_token': MINTED, 'expires_in': 3600}))
            calls['api'] += 1
            if calls['api'] == 1:
                raise urllib.error.HTTPError(req.full_url, 401, 'no', {}, io.BytesIO(b'{}'))
            return FakeResponse('{"ok": true}')

        code, out, _ = self.call(fake_open)
        self.assertEqual((code, calls['api'], calls['mint']), (0, 2, 1))

    def test_only_the_stored_region_host_gets_the_credential(self):
        self.link()
        for url in ('https://sellingpartnerapi-eu.amazon.com/x', 'https://sandbox.sellingpartnerapi-na.amazon.com/x',
                    'https://evil.example/x', f'https://{HOST}.evil.example/x', 'http://' + HOST + '/x'):
            with self.subTest(url=url):
                calls, fake = self.opener()
                code, _, _ = self.call(fake, url)
                self.assertNotEqual(code, 0)
                self.assertEqual(calls['api'], [])
                self.assertEqual(calls['mint'], [])

    def test_a_tampered_stored_host_is_never_used(self):
        for bad in ('evil.example', 'sellingpartnerapi-na.amazon.com.evil.example', 'api.amazon.com', ''):
            with self.subTest(host=bad):
                self.link(host=bad)
                calls, fake = self.opener()
                code, _, _ = self.call(fake)
                self.assertNotEqual(code, 0)
                self.assertEqual(calls['api'] + calls['mint'], [])

    def test_sandbox_host_is_accepted_when_it_is_the_stored_one(self):
        self.link(host='sandbox.sellingpartnerapi-na.amazon.com')
        calls, fake = self.opener()
        code, _, _ = self.call(fake, 'https://sandbox.sellingpartnerapi-na.amazon.com/x')
        self.assertEqual(code, 0)

    def test_caller_cannot_set_the_amazon_header(self):
        self.link()
        calls, fake = self.opener()
        code, _, err = self.call(fake, API, 'GET', '--header', 'x-amz-access-token=evil')
        self.assertNotEqual(code, 0)
        self.assertEqual(calls['api'], [])

    def test_nothing_secret_reaches_the_output_even_if_the_api_echoes_it(self):
        self.link(expires_in=-10)
        _, fake = self.opener(api=lambda req: FakeResponse(json.dumps({'echo': [MINTED, CLIENT_SECRET, REFRESH]})))
        code, out, err = self.call(fake)
        self.assertEqual(code, 0)
        for secret in (MINTED, CLIENT_SECRET, REFRESH):
            self.assertNotIn(secret, out + err)

    def test_info_shows_the_host_and_saved_state_without_values(self):
        self.link()
        code, out, _ = self.run_main('info', 'amazon_sp')
        self.assertEqual(code, 0)
        info = json.loads(out)
        self.assertEqual(info['api_host'], HOST)
        self.assertEqual(info['client_credentials'], 'saved')
        for secret in (CLIENT_SECRET, REFRESH, TOKEN):
            self.assertNotIn(secret, out)

    def test_info_reports_missing_when_the_refresh_token_is_gone(self):
        self.link(refresh='')
        _, out, _ = self.run_main('info', 'amazon_sp')
        self.assertEqual(json.loads(out)['client_credentials'], 'missing')

    def test_not_connected(self):
        calls, fake = self.opener()
        code, _, err = self.call(fake)
        self.assertEqual(code, 3)
        self.assertIn('not connected', err)


if __name__ == '__main__':
    unittest.main()
