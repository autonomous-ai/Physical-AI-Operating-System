"""Behavior tests for the connector helper; no network."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import time
import unittest
import urllib.error
from unittest.mock import patch

SCRIPT = Path(__file__).parents[1] / 'scripts/connector.py'
TOKEN = 'tok-' + 'x' * 40
CLIENT_ID = 'cid-' + 'a' * 12
CLIENT_SECRET = 'shpss_' + 'z' * 32
MINTED = 'minted-' + 'm' * 30


def load_module(configs_dir):
    with patch.dict(os.environ, {'CONNECTOR_CONFIGS_DIR': str(configs_dir)}):
        spec = importlib.util.spec_from_file_location('connector_skill', SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
    return module


class FakeResponse:
    def __init__(self, body, status=200):
        self.body = body.encode()
        self.status = status

    def read(self, _limit=None):
        return self.body

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


class HelperTestCase(unittest.TestCase):
    """A temporary configs folder and a freshly loaded helper module."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.dir = Path(self.temp.name)
        self.mod = load_module(self.dir)

    def write(self, code, entry):
        (self.dir / f'{code}_access_tokens.json').write_text(json.dumps({'connectors': {code: entry}}))

    def run_main(self, *argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = self.mod.main(list(argv))
        return code, out.getvalue(), err.getvalue()


class ConnectorHelperTests(HelperTestCase):
    def setUp(self):
        super().setUp()
        self.write('google_calendar', {
            'auth_type': 'oauth', 'access_token': TOKEN, 'refresh_token': 'r', 'refresh': True,
            'expires_at': int(time.time()) + 3600, 'user_email': 'me@example.com',
            'scopes': ['https://www.googleapis.com/auth/calendar.events'],
        })

    def test_list_names_connector_without_secret(self):
        code, out, _ = self.run_main('list')
        self.assertEqual(code, 0)
        self.assertIn('google_calendar: connected (me@example.com, oauth)', out)
        self.assertNotIn(TOKEN, out)

    def test_list_reports_unreadable_file_as_verification_failure(self):
        (self.dir / 'broken_access_tokens.json').write_text('{not json')
        code, _, err = self.run_main('list')
        self.assertEqual(code, 3)
        self.assertIn('verification failed', err)

    def test_info_has_no_secret(self):
        code, out, _ = self.run_main('info', 'google_calendar')
        self.assertEqual(code, 0)
        info = json.loads(out)
        self.assertTrue(info['auto_refresh'])
        self.assertFalse(info['expired'])
        self.assertNotIn(TOKEN, out)

    def test_call_sends_header_and_encodes_query(self):
        seen = {}

        def fake_urlopen(req, timeout):
            seen['url'] = req.full_url
            seen['auth'] = req.get_header('Authorization')
            return FakeResponse('{"items": []}')

        with patch.object(self.mod._OPENER, 'open', fake_urlopen):
            code, out, _ = self.run_main(
                'call', 'google_calendar', 'GET',
                'https://www.googleapis.com/calendar/v3/calendars/primary/events',
                '--query', 'timeMin=2026-07-13T00:00:00+07:00')
        self.assertEqual(code, 0)
        self.assertEqual(seen['auth'], 'Bearer ' + TOKEN)
        self.assertIn('timeMin=2026-07-13T00%3A00%3A00%2B07%3A00', seen['url'])
        self.assertEqual(json.loads(out), {'items': []})

    def test_http_error_goes_to_stderr_only(self):
        def fake_urlopen(req, timeout):
            raise urllib.error.HTTPError(req.full_url, 401, 'Unauthorized', {}, io.BytesIO(b'{"error":{"code":401}}'))

        with patch.object(self.mod._OPENER, 'open', fake_urlopen):
            code, out, err = self.run_main('call', 'google_calendar', 'GET', 'https://www.googleapis.com/x')
        self.assertEqual(code, 1)
        self.assertEqual(out, '')
        self.assertIn('HTTP 401 from www.googleapis.com', err)
        self.assertIn('auto_refresh=yes', err)

    def test_refuses_foreign_host_for_known_connector(self):
        with patch.object(self.mod._OPENER, 'open') as urlopen:
            code, _, err = self.run_main('call', 'google_calendar', 'GET', 'https://evil.example.com/collect')
        self.assertEqual(code, 4)
        self.assertIn('refusing', err)
        urlopen.assert_not_called()

    def test_refuses_lookalike_host(self):
        code, _, _ = self.run_main('call', 'google_calendar', 'GET', 'https://googleapis.com.evil.example/x')
        self.assertEqual(code, 4)

    def test_refuses_plain_http(self):
        code, _, _ = self.run_main('call', 'google_calendar', 'GET', 'http://www.googleapis.com/x')
        self.assertEqual(code, 4)

    def test_refuses_caller_authorization_header(self):
        code, _, err = self.run_main('call', 'google_calendar', 'GET', 'https://www.googleapis.com/x',
                                     '--header', 'Authorization: Basic abc')
        self.assertEqual(code, 2)
        self.assertIn('Authorization', err)

    def test_google_app_password_is_refused(self):
        self.write('gmail', {'auth_type': 'pat', 'api_key': 'app-pass', 'credentials': {'email': 'me@example.com'}})
        code, _, err = self.run_main('call', 'gmail', 'GET', 'https://gmail.googleapis.com/gmail/v1/users/me/messages')
        self.assertEqual(code, 3)
        self.assertIn('IMAP', err)

    def test_not_connected(self):
        code, _, err = self.run_main('call', 'google_drive', 'GET', 'https://www.googleapis.com/drive/v3/files')
        self.assertEqual(code, 3)
        self.assertIn('not connected', err)

    def test_token_param_uses_query_not_header(self):
        self.write('facebook', {'api_key': TOKEN, 'credentials': {'page_id': '123'}})
        seen = {}

        def fake_urlopen(req, timeout):
            seen['url'] = req.full_url
            seen['auth'] = req.get_header('Authorization')
            return FakeResponse('{"success": true}')

        with patch.object(self.mod._OPENER, 'open', fake_urlopen):
            code, out, _ = self.run_main('call', 'facebook', 'DELETE', 'https://graph.facebook.com/v19.0/1_2',
                                         '--token-param', 'access_token')
        self.assertEqual(code, 0)
        self.assertIsNone(seen['auth'])
        self.assertIn('access_token=' + TOKEN, seen['url'])
        self.assertNotIn(TOKEN, out)

    def test_response_echoing_token_is_scrubbed(self):
        with patch.object(self.mod._OPENER, 'open', lambda req, timeout: FakeResponse(TOKEN)):
            code, out, _ = self.run_main('call', 'google_calendar', 'GET', 'https://www.googleapis.com/x')
        self.assertEqual(code, 0)
        self.assertNotIn(TOKEN, out)

    def test_unlisted_connector_is_refused(self):
        self.write('mystery', {'access_token': TOKEN})
        with patch.object(self.mod._OPENER, 'open') as opener:
            code, _, err = self.run_main('call', 'mystery', 'GET', 'https://attacker.example/collect')
        self.assertEqual(code, 4)
        self.assertIn('no official API host', err)
        opener.assert_not_called()

    def test_userinfo_and_custom_port_are_refused(self):
        for url in ('https://evil.example@www.googleapis.com/x', 'https://www.googleapis.com:8443/x'):
            code, _, _ = self.run_main('call', 'google_calendar', 'GET', url)
            self.assertEqual(code, 4, url)

    def test_token_param_only_for_facebook(self):
        code, _, err = self.run_main('call', 'google_calendar', 'GET', 'https://www.googleapis.com/x',
                                     '--token-param', 'access_token')
        self.assertEqual(code, 2)
        self.assertIn('--token-param', err)

    def test_redirect_is_not_followed(self):
        handler = self.mod._NoRedirect()
        req = self.mod.urllib.request.Request('https://www.googleapis.com/x', headers={'Authorization': 'Bearer ' + TOKEN})
        self.assertIsNone(handler.redirect_request(req, None, 302, 'Found', {}, 'https://attacker.example/'))

    # --- Shopify: per-account host, own auth header ---

    SHOP = 'my-shop.myshopify.com'

    def link_shopify(self, **credentials):
        # The shape the device software stores for a customer-app connector: a
        # PAT-style entry holding a minted token plus the client credentials.
        credentials.setdefault('client_id', CLIENT_ID)
        credentials.setdefault('client_secret', CLIENT_SECRET)
        self.write('shopify', {
            'auth_type': 'pat', 'access_token': TOKEN,
            'expires_at': int(time.time()) + 3600, 'scopes': ['write_products'],
            'credentials': credentials,
        })

    def test_shopify_sends_raw_token_in_shopify_header_to_stored_shop(self):
        self.link_shopify(shop=self.SHOP)
        seen = {}

        def fake_urlopen(req, timeout):
            seen['url'] = req.full_url
            seen['token'] = req.get_header('X-shopify-access-token')
            seen['auth'] = req.get_header('Authorization')
            seen['body'] = req.data
            return FakeResponse('{"data": {}}')

        with patch.object(self.mod._OPENER, 'open', fake_urlopen):
            code, out, _ = self.run_main(
                'call', 'shopify', 'POST', 'https://%s/admin/api/2025-10/graphql.json' % self.SHOP,
                '--json', '{"query": "{ shop { name } }"}')
        self.assertEqual(code, 0)
        self.assertEqual(seen['token'], TOKEN)
        self.assertIsNone(seen['auth'])
        self.assertEqual(seen['url'], 'https://%s/admin/api/2025-10/graphql.json' % self.SHOP)
        self.assertEqual(json.loads(out), {'data': {}})

    def test_shopify_host_comes_from_connector_vars_when_shop_key_is_absent(self):
        self.link_shopify(connector_vars=json.dumps({'shop': self.SHOP}))
        with patch.object(self.mod._OPENER, 'open', lambda req, timeout: FakeResponse('{}')):
            code, _, _ = self.run_main('call', 'shopify', 'GET', 'https://%s/admin/api/2025-10/shop.json' % self.SHOP)
        self.assertEqual(code, 0)

    def test_shopify_refuses_other_shop_and_other_hosts(self):
        self.link_shopify(shop=self.SHOP)
        for url in (
            'https://other-shop.myshopify.com/admin/api/2025-10/graphql.json',  # another store
            'https://myshopify.com/admin',                                      # the bare family domain
            'https://%s.evil.example/x' % self.SHOP,                            # lookalike
            'https://evil.example/%s' % self.SHOP,
            'https://admin.shopify.com/store/my-shop',                          # a real Shopify host that is not the stored shop
        ):
            with self.subTest(url=url), patch.object(self.mod._OPENER, 'open') as urlopen:
                code, _, err = self.run_main('call', 'shopify', 'GET', url)
                self.assertEqual(code, 4)
                self.assertNotIn(TOKEN, err)
                urlopen.assert_not_called()

    def test_shopify_without_a_valid_stored_shop_is_unusable(self):
        for credentials in ({}, {'shop': ''}, {'shop': 'evil.example.com'}, {'shop': 'a.myshopify.com.evil.example'},
                            {'shop': 'a/b.myshopify.com'}, {'connector_vars': 'not json'}):
            with self.subTest(credentials=credentials):
                self.link_shopify(**credentials)
                with patch.object(self.mod._OPENER, 'open') as urlopen:
                    code, _, err = self.run_main('call', 'shopify', 'GET', 'https://my-shop.myshopify.com/admin/api/2025-10/shop.json')
                self.assertEqual(code, 3)
                self.assertIn('reconnect', err)
                urlopen.assert_not_called()

    def test_shopify_not_connected(self):
        code, _, err = self.run_main('call', 'shopify', 'GET', 'https://my-shop.myshopify.com/admin/api/2025-10/shop.json')
        self.assertEqual(code, 3)
        self.assertIn('not connected', err)

    def test_shopify_refuses_caller_supplied_credential_header(self):
        self.link_shopify(shop=self.SHOP)
        for header in ('X-Shopify-Access-Token: stolen', 'x-shopify-access-token: stolen', 'X-Shopify-Access-Token : stolen',
                       'Authorization: Bearer x'):
            with self.subTest(header=header), patch.object(self.mod._OPENER, 'open') as urlopen:
                code, _, err = self.run_main('call', 'shopify', 'GET', 'https://%s/admin/api/2025-10/shop.json' % self.SHOP,
                                             '--header', header)
                self.assertEqual(code, 2)
                self.assertIn('do not pass', err)
                self.assertNotIn('stolen', err)
                urlopen.assert_not_called()

    def test_header_injection_is_refused(self):
        for header in ('X-Note: ok\r\nX-Evil: 1', 'X-Note\nX-Evil: 1'):
            with self.subTest(header=header), patch.object(self.mod._OPENER, 'open') as urlopen:
                code, _, err = self.run_main('call', 'google_calendar', 'GET', 'https://www.googleapis.com/x',
                                             '--header', header)
                self.assertEqual(code, 2)
                self.assertIn('line breaks', err)
                urlopen.assert_not_called()

    def test_shopify_info_and_list_show_shop_without_secret(self):
        self.link_shopify(shop=self.SHOP)
        code, out, _ = self.run_main('info', 'shopify')
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out)['shop'], self.SHOP)
        self.assertNotIn(TOKEN, out)
        code, out, _ = self.run_main('list')
        self.assertEqual(code, 0)
        self.assertIn('shopify: connected (%s, pat)' % self.SHOP, out)
        self.assertNotIn(CLIENT_SECRET, out)
        self.assertNotIn(TOKEN, out)

    def test_shopify_info_flags_missing_shop(self):
        self.link_shopify()
        code, out, _ = self.run_main('info', 'shopify')
        self.assertEqual(code, 0)
        self.assertIn('problem', json.loads(out))
        self.assertNotIn('shop', json.loads(out))

    def test_shopify_error_body_never_echoes_token(self):
        # No saved client credentials: the helper cannot mint a replacement, so
        # the 401 is reported as is.
        self.link_shopify(shop=self.SHOP, client_id='', client_secret='')

        def fake_urlopen(req, timeout):
            raise urllib.error.HTTPError(req.full_url, 401, 'Unauthorized', {},
                                         io.BytesIO(('{"errors":"bad token %s"}' % TOKEN).encode()))

        with patch.object(self.mod._OPENER, 'open', fake_urlopen):
            code, out, err = self.run_main('call', 'shopify', 'GET', 'https://%s/admin/api/2025-10/shop.json' % self.SHOP)
        self.assertEqual(code, 1)
        self.assertEqual(out, '')
        self.assertNotIn(TOKEN, err)
        self.assertIn('[credential]', err)

    def test_upload_refuses_credentials_and_non_media(self):
        token_file = self.dir / 'google_calendar_access_tokens.json'
        alias = self.dir / 'innocent.jpg'
        alias.symlink_to(token_file)
        for raw in (str(token_file), str(alias), '/root/config/config.json'):
            with self.assertRaises(self.mod.Failure, msg=raw):
                self.mod.upload_path(raw)
        image = self.dir / 'pic.png'
        image.write_bytes(b'png')
        self.assertEqual(self.mod.upload_path(str(image)), image.resolve())

    def test_scrub_covers_url_encoded_token(self):
        token = 'a/b+c=='
        text = 'x ' + token + ' y a%2Fb%2Bc%3D%3D z'
        self.assertNotIn('a%2Fb', self.mod.scrub(text, token))
        self.assertNotIn(token, self.mod.scrub(text, token))

    def test_multipart_upload(self):
        image = self.dir / 'pic.jpg'
        image.write_bytes(b'\xff\xd8data')
        body, ctype = self.mod.multipart([('source', '@' + str(image)), ('message', 'hi')])
        self.assertTrue(ctype.startswith('multipart/form-data; boundary='))
        self.assertIn(b'filename="pic.jpg"', body)
        self.assertIn(b'\xff\xd8data', body)
        self.assertIn(b'name="message"\r\n\r\nhi', body)


class ShopifyClientCredentialsTests(HelperTestCase):
    """The customer's own app: the helper mints short-lived tokens itself."""

    SHOP = 'my-shop.myshopify.com'
    API = 'https://my-shop.myshopify.com/admin/api/2025-10/graphql.json'

    def link(self, token=TOKEN, expires_in=3600, shop=None, client_id=CLIENT_ID, client_secret=CLIENT_SECRET, **extra):
        credentials = {'shop': shop if shop is not None else self.SHOP}
        if client_id:
            credentials['client_id'] = client_id
        if client_secret:
            credentials['client_secret'] = client_secret
        credentials.update(extra)
        entry = {'auth_type': 'pat', 'access_token': token, 'credentials': credentials,
                 'expires_at': int(time.time()) + expires_in if token else 0}
        self.write('shopify', entry)

    def cache_file(self):
        return self.dir / 'shopify.token.json'

    def opener(self, mint=None, api=None):
        """A fake network: records every request and answers token requests with
        `mint` and API calls with `api` (each a callable, an exception or a body)."""
        calls = {'mint': [], 'api': []}

        def respond(handler, req):
            if isinstance(handler, BaseException):
                raise handler
            if callable(handler):
                return handler(req)
            return FakeResponse(handler)

        def fake_open(req, timeout):
            if req.full_url.endswith('/admin/oauth/access_token'):
                calls['mint'].append(req)
                return respond(mint if mint is not None else json.dumps({'access_token': MINTED, 'expires_in': 86399}), req)
            calls['api'].append(req)
            return respond(api if api is not None else '{"data": {}}', req)

        return calls, fake_open

    def call(self, fake_open, url=None, *extra):
        with patch.object(self.mod._OPENER, 'open', fake_open):
            return self.run_main('call', 'shopify', 'POST', url or self.API, '--json', '{"query": "{ shop { name } }"}', *extra)

    @staticmethod
    def http_error(url, code, body=b''):
        return urllib.error.HTTPError(url, code, 'err', {}, io.BytesIO(body))

    def test_valid_stored_token_is_used_without_minting(self):
        self.link()
        calls, fake = self.opener()
        code, _, _ = self.call(fake)
        self.assertEqual(code, 0)
        self.assertEqual(calls['mint'], [])
        self.assertEqual(calls['api'][0].get_header('X-shopify-access-token'), TOKEN)

    def test_expired_token_is_minted_cached_and_used(self):
        self.link(expires_in=-60)
        entry_before = (self.dir / 'shopify_access_tokens.json').read_bytes()
        calls, fake = self.opener()
        code, out, err = self.call(fake)
        self.assertEqual(code, 0)
        self.assertEqual(len(calls['mint']), 1)
        self.assertEqual(calls['api'][0].get_header('X-shopify-access-token'), MINTED)
        cache = json.loads(self.cache_file().read_text())
        self.assertEqual(cache['access_token'], MINTED)
        self.assertGreater(cache['expires_at'], time.time() + 86000)
        self.assertEqual(self.cache_file().stat().st_mode & 0o777, 0o600)
        # The entry the device software manages is never rewritten by the helper.
        self.assertEqual((self.dir / 'shopify_access_tokens.json').read_bytes(), entry_before)
        for text in (out, err):
            self.assertNotIn(MINTED, text)

    def test_mint_request_goes_to_the_stored_shop_with_the_client_credentials(self):
        self.link(expires_in=-60)
        calls, fake = self.opener()
        self.call(fake)
        req = calls['mint'][0]
        self.assertEqual(req.full_url, 'https://%s/admin/oauth/access_token' % self.SHOP)
        self.assertEqual(req.get_method(), 'POST')
        self.assertIsNone(req.get_header('Authorization'))
        self.assertIsNone(req.get_header('X-shopify-access-token'))
        form = dict(urllib_parse_qsl(req.data.decode()))
        self.assertEqual(form, {'grant_type': 'client_credentials', 'client_id': CLIENT_ID, 'client_secret': CLIENT_SECRET})

    def test_cached_token_is_reused_on_the_next_call(self):
        self.link(expires_in=-60)
        calls, fake = self.opener()
        self.call(fake)
        self.call(fake)
        self.assertEqual(len(calls['mint']), 1)
        self.assertEqual(len(calls['api']), 2)
        self.assertEqual(calls['api'][1].get_header('X-shopify-access-token'), MINTED)

    def test_missing_token_is_minted(self):
        self.link(token='')
        calls, fake = self.opener()
        code, _, _ = self.call(fake)
        self.assertEqual(code, 0)
        self.assertEqual(len(calls['mint']), 1)

    def test_token_close_to_expiry_is_renewed(self):
        self.link(expires_in=120)
        calls, fake = self.opener()
        self.call(fake)
        self.assertEqual(len(calls['mint']), 1)
        self.assertEqual(calls['api'][0].get_header('X-shopify-access-token'), MINTED)

    def test_cache_is_ignored_when_the_client_id_changes(self):
        self.link(expires_in=-60)
        calls, fake = self.opener()
        self.call(fake)
        self.link(expires_in=-60, client_id='cid-other')
        self.call(fake)
        self.assertEqual(len(calls['mint']), 2)

    def test_cache_is_ignored_when_the_shop_changes(self):
        self.link(expires_in=-60)
        calls, fake = self.opener()
        self.call(fake)
        self.link(expires_in=-60, shop='other-shop.myshopify.com')
        self.call(fake, 'https://other-shop.myshopify.com/admin/api/2025-10/graphql.json')
        self.assertEqual(len(calls['mint']), 2)
        self.assertEqual(calls['mint'][1].full_url, 'https://other-shop.myshopify.com/admin/oauth/access_token')

    def test_cache_holds_no_secret_and_no_plain_client_id(self):
        self.link(expires_in=-60)
        _, fake = self.opener()
        self.call(fake)
        text = self.cache_file().read_text()
        self.assertNotIn(CLIENT_SECRET, text)
        self.assertNotIn(CLIENT_ID, text)

    def test_damaged_cache_is_just_a_miss(self):
        self.link(expires_in=-60)
        self.cache_file().write_text('{not json')
        calls, fake = self.opener()
        code, _, _ = self.call(fake)
        self.assertEqual(code, 0)
        self.assertEqual(len(calls['mint']), 1)

    def test_failing_to_write_the_cache_is_not_fatal(self):
        self.link(expires_in=-60)
        calls, fake = self.opener()
        with patch.object(self.mod.os, 'replace', side_effect=OSError('disk full')):
            code, _, _ = self.call(fake)
        self.assertEqual(code, 0)
        self.assertFalse(self.cache_file().exists())
        self.assertEqual([p.name for p in self.dir.glob('.*.tmp')], [])

    def test_cache_is_dropped_when_the_connector_is_gone(self):
        self.link(expires_in=-60)
        _, fake = self.opener()
        self.call(fake)
        self.assertTrue(self.cache_file().exists())
        (self.dir / 'shopify_access_tokens.json').unlink()
        code, _, err = self.call(fake)
        self.assertEqual(code, 3)
        self.assertIn('not connected', err)
        self.assertFalse(self.cache_file().exists())

    def test_minted_lifetime_is_bounded_and_defaulted(self):
        self.link(expires_in=-60)
        _, fake = self.opener(mint=json.dumps({'access_token': MINTED, 'expires_in': 10 ** 9}))
        self.call(fake)
        self.assertLessEqual(json.loads(self.cache_file().read_text())['expires_at'], time.time() + 7 * 86400 + 5)
        self.cache_file().unlink()
        _, fake = self.opener(mint=json.dumps({'access_token': MINTED}))
        self.call(fake)
        expires_at = json.loads(self.cache_file().read_text())['expires_at']
        self.assertAlmostEqual(expires_at, time.time() + 3600, delta=10)

    def test_rejected_token_is_reminted_once_and_the_call_retried(self):
        self.link()
        state = {'n': 0}

        def api(req):
            state['n'] += 1
            if state['n'] == 1:
                raise self.http_error(req.full_url, 401, b'{"errors":"invalid"}')
            return FakeResponse('{"data": {"ok": true}}')

        calls, fake = self.opener(api=api)
        code, out, _ = self.call(fake)
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out), {'data': {'ok': True}})
        self.assertEqual(len(calls['mint']), 1)
        self.assertEqual([r.get_header('X-shopify-access-token') for r in calls['api']], [TOKEN, MINTED])

    def test_a_token_minted_in_this_call_is_not_retried_on_401(self):
        self.link(expires_in=-60)
        calls, fake = self.opener(api=lambda req: (_ for _ in ()).throw(self.http_error(req.full_url, 401)))
        code, out, err = self.call(fake)
        self.assertEqual(code, 1)
        self.assertEqual(out, '')
        self.assertIn('HTTP 401', err)
        self.assertEqual((len(calls['mint']), len(calls['api'])), (1, 1))

    def test_a_second_401_after_the_retry_stops(self):
        self.link()
        calls, fake = self.opener(api=lambda req: (_ for _ in ()).throw(self.http_error(req.full_url, 401)))
        code, _, err = self.call(fake)
        self.assertEqual(code, 1)
        self.assertIn('HTTP 401', err)
        self.assertEqual((len(calls['mint']), len(calls['api'])), (1, 2))

    def test_401_without_saved_client_credentials_is_reported_not_retried(self):
        self.link(client_id='', client_secret='')
        calls, fake = self.opener(api=lambda req: (_ for _ in ()).throw(self.http_error(req.full_url, 401)))
        code, _, err = self.call(fake)
        self.assertEqual(code, 1)
        self.assertIn('HTTP 401', err)
        self.assertEqual((len(calls['mint']), len(calls['api'])), (0, 1))

    def test_rejected_credentials_ask_to_reconnect_without_echoing_anything(self):
        self.link(expires_in=-60)
        body = ('<html>invalid_request client %s secret %s</html>' % (CLIENT_ID, CLIENT_SECRET)).encode()
        for status in (400, 401, 403):
            with self.subTest(status=status):
                calls, fake = self.opener(mint=lambda req: (_ for _ in ()).throw(self.http_error(req.full_url, status, body)))
                code, out, err = self.call(fake)
                self.assertEqual(code, 3)
                self.assertEqual(out, '')
                self.assertIn('reconnect shopify', err)
                self.assertIn('rejected the saved client ID/secret', err)
                for secret in (CLIENT_SECRET, CLIENT_ID):
                    self.assertNotIn(secret, err)
                self.assertEqual(calls['api'], [])

    def test_network_failures_are_retryable(self):
        self.link(expires_in=-60)
        cases = {
            'unreachable': urllib.error.URLError(OSError('no route')),
            'server error': self.http_error('https://x', 503),
            'garbage body': '<html>not json</html>',
            'no token': json.dumps({'expires_in': 100}),
            'empty token': json.dumps({'access_token': '', 'expires_in': 100}),
            'oversized lifetime string': json.dumps({'access_token': MINTED, 'expires_in': 'soon'}),
        }
        for name, mint in cases.items():
            with self.subTest(case=name):
                calls, fake = self.opener(mint=mint)
                code, out, err = self.call(fake)
                self.assertEqual(code, 1)
                self.assertEqual(out, '')
                self.assertIn('try again', err)
                self.assertNotIn(CLIENT_SECRET, err)
                self.assertEqual(calls['api'], [])

    def test_a_redirected_token_request_is_not_followed(self):
        self.link(expires_in=-60)
        calls, fake = self.opener(mint=lambda req: (_ for _ in ()).throw(self.http_error(req.full_url, 302)))
        code, _, err = self.call(fake)
        self.assertEqual(code, 1)
        self.assertIn('not followed', err)
        self.assertEqual(calls['api'], [])

    def test_expired_token_without_client_credentials_asks_to_reconnect(self):
        self.link(expires_in=-60, client_id='', client_secret='')
        calls, fake = self.opener()
        code, _, err = self.call(fake)
        self.assertEqual(code, 3)
        self.assertIn('reconnect shopify', err)
        self.assertEqual((calls['mint'], calls['api']), ([], []))

    def test_a_tampered_shop_never_receives_the_client_secret(self):
        for shop in ('evil.example.com', 'a.myshopify.com.evil.example', 'a/b.myshopify.com', 'my-shop.myshopify.com.evil.example', ''):
            with self.subTest(shop=shop):
                self.link(expires_in=-60, shop=shop)
                calls, fake = self.opener()
                code, _, err = self.call(fake)
                self.assertNotEqual(code, 0)
                self.assertEqual((calls['mint'], calls['api']), ([], []))
                self.assertNotIn(CLIENT_SECRET, err)

    def test_case_and_padding_of_the_stored_shop_are_normalized_to_the_same_host(self):
        self.link(shop=' My-Shop.MyShopify.com ')
        calls, fake = self.opener()
        code, _, _ = self.call(fake)
        self.assertEqual(code, 0)
        self.assertEqual(calls['api'][0].full_url, self.API)

    def test_calling_another_host_never_triggers_a_token_request(self):
        self.link(expires_in=-60)
        for url in ('https://other-shop.myshopify.com/admin/api/2025-10/graphql.json',
                    'https://evil.example/admin/oauth/access_token',
                    'https://admin.shopify.com/store/my-shop'):
            with self.subTest(url=url):
                calls, fake = self.opener()
                code, _, _ = self.call(fake, url)
                self.assertEqual(code, 4)
                self.assertEqual((calls['mint'], calls['api']), ([], []))

    def test_secret_and_tokens_never_reach_stdout_or_stderr(self):
        self.link(expires_in=-60)
        echo = '{"echo": "%s %s %s"}' % (MINTED, CLIENT_SECRET, CLIENT_ID)
        _, fake = self.opener(api=echo)
        code, out, err = self.call(fake)
        self.assertEqual(code, 0)
        for secret in (MINTED, CLIENT_SECRET):
            self.assertNotIn(secret, out + err)
        self.assertIn('[credential]', out)
        # And on an error body that echoes them.
        _, fake = self.opener(api=lambda req: (_ for _ in ()).throw(self.http_error(req.full_url, 500, echo.encode())))
        code, out, err = self.call(fake)
        self.assertEqual(code, 1)
        for secret in (MINTED, CLIENT_SECRET):
            self.assertNotIn(secret, out + err)

    def test_info_reports_saved_credentials_without_values(self):
        self.link(expires_in=-60)
        code, out, _ = self.run_main('info', 'shopify')
        self.assertEqual(code, 0)
        info = json.loads(out)
        self.assertEqual(info['shop'], self.SHOP)
        self.assertEqual(info['client_credentials'], 'saved')
        self.assertTrue(info['auto_refresh'])
        self.assertFalse(info['expired'])
        for secret in (CLIENT_SECRET, CLIENT_ID, TOKEN):
            self.assertNotIn(secret, out)
        self.link(client_id='', client_secret='')
        info = json.loads(self.run_main('info', 'shopify')[1])
        self.assertEqual(info['client_credentials'], 'missing')

    def test_list_shows_shopify_and_ignores_the_token_cache(self):
        self.link(expires_in=-60)
        _, fake = self.opener()
        self.call(fake)
        code, out, _ = self.run_main('list')
        self.assertEqual(code, 0)
        self.assertEqual(out.strip().splitlines(), ['shopify: connected (%s, pat)' % self.SHOP])
        self.assertNotIn(MINTED, out)

    def test_scrub_covers_several_secrets(self):
        text = 'a %s b %s c' % (TOKEN, CLIENT_SECRET)
        scrubbed = self.mod.scrub(text, TOKEN, CLIENT_SECRET, '')
        self.assertNotIn(TOKEN, scrubbed)
        self.assertNotIn(CLIENT_SECRET, scrubbed)


EBAY_REFRESH = 'v^1.1#refresh-' + 'r' * 30
EBAY_SECRET = 'PRD-secret-' + 's' * 20
EBAY_ACCESS = 'v^1.1#access-' + 'a' * 30
EBAY_INVENTORY = 'https://api.ebay.com/sell/inventory/v1/inventory_item'


class EbayConnectorTests(HelperTestCase):
    """The customer's own eBay app: access tokens minted from a refresh token."""

    def setUp(self):
        super().setUp()
        self.link()
        self.token_calls = []
        self.api_calls = []

    def link(self, obtained_at=1000, **creds):
        credentials = {'client_id': 'App-PRD-123', 'client_secret': EBAY_SECRET, 'marketplace_id': 'EBAY_GB'}
        credentials.update(creds)
        self.write('ebay', {'auth_type': 'pat', 'api_key': EBAY_REFRESH, 'credentials': credentials,
                            'obtained_at': obtained_at})

    def fake_open(self, api_status=200, api_body='{"inventoryItems": []}', token_error=None):
        def opener(req, timeout):
            if req.full_url.endswith('/identity/v1/oauth2/token'):
                self.token_calls.append(req)
                if token_error:
                    raise urllib.error.HTTPError(req.full_url, 400, 'Bad Request', {},
                                                 io.BytesIO(json.dumps({'error': token_error}).encode()))
                return FakeResponse(json.dumps({'access_token': EBAY_ACCESS + str(len(self.token_calls)),
                                                'expires_in': 7200}))
            self.api_calls.append(req)
            status = api_status(len(self.api_calls)) if callable(api_status) else api_status
            if status >= 400:
                raise urllib.error.HTTPError(req.full_url, status, 'err', {}, io.BytesIO(b'{"errors": []}'))
            return FakeResponse(api_body)
        return opener

    def test_call_mints_token_and_sets_marketplace_headers(self):
        with patch.object(self.mod._OPENER, 'open', self.fake_open()):
            code, out, _ = self.run_main('call', 'ebay', 'PUT', EBAY_INVENTORY + '/SKU-1', '--json', '{"product": {}}')
        self.assertEqual(code, 0)
        token_req = self.token_calls[0]
        self.assertEqual(token_req.full_url, 'https://api.ebay.com/identity/v1/oauth2/token')
        self.assertTrue(token_req.get_header('Authorization').startswith('Basic '))
        self.assertIn(b'grant_type=refresh_token', token_req.data)
        api_req = self.api_calls[0]
        self.assertEqual(api_req.get_header('Authorization'), 'Bearer ' + EBAY_ACCESS + '1')
        self.assertEqual(api_req.get_header('X-ebay-c-marketplace-id'), 'EBAY_GB')
        self.assertEqual(api_req.get_header('Content-language'), 'en-GB')
        self.assertEqual(json.loads(out), {'inventoryItems': []})

    def test_refresh_token_is_never_sent_as_access_token(self):
        with patch.object(self.mod._OPENER, 'open', self.fake_open()):
            self.run_main('call', 'ebay', 'GET', EBAY_INVENTORY)
        self.assertNotIn(EBAY_REFRESH, self.api_calls[0].get_header('Authorization'))

    def test_cached_token_is_reused(self):
        with patch.object(self.mod._OPENER, 'open', self.fake_open()):
            self.run_main('call', 'ebay', 'GET', EBAY_INVENTORY)
            self.run_main('call', 'ebay', 'GET', EBAY_INVENTORY)
        self.assertEqual(len(self.token_calls), 1)
        self.assertEqual(len(self.api_calls), 2)
        self.assertEqual((self.dir / 'ebay.token.json').stat().st_mode & 0o777, 0o600)

    def test_relinking_ignores_cached_token(self):
        with patch.object(self.mod._OPENER, 'open', self.fake_open()):
            self.run_main('call', 'ebay', 'GET', EBAY_INVENTORY)
            self.link(obtained_at=2000)
            self.run_main('call', 'ebay', 'GET', EBAY_INVENTORY)
        self.assertEqual(len(self.token_calls), 2)

    def test_rejected_cached_token_is_minted_again_once(self):
        with patch.object(self.mod._OPENER, 'open', self.fake_open()):
            self.run_main('call', 'ebay', 'GET', EBAY_INVENTORY)
        self.api_calls.clear()
        with patch.object(self.mod._OPENER, 'open', self.fake_open(api_status=lambda n: 401 if n == 1 else 200)):
            code, _, _ = self.run_main('call', 'ebay', 'GET', EBAY_INVENTORY)
        self.assertEqual(code, 0)
        self.assertEqual(self.api_calls[-1].get_header('Authorization'), 'Bearer ' + EBAY_ACCESS + '2')

    def test_revoked_refresh_token_asks_to_reconnect(self):
        with patch.object(self.mod._OPENER, 'open', self.fake_open(token_error='invalid_grant')):
            code, out, err = self.run_main('call', 'ebay', 'GET', EBAY_INVENTORY)
        self.assertEqual(code, 3)
        self.assertEqual(out, '')
        self.assertIn('refresh token', err)
        self.assertEqual(self.api_calls, [])

    def test_sandbox_host_refused_for_production_link(self):
        with patch.object(self.mod._OPENER, 'open') as opener:
            code, _, err = self.run_main('call', 'ebay', 'GET', 'https://api.sandbox.ebay.com/sell/inventory/v1/inventory_item')
        self.assertEqual(code, 4)
        self.assertIn('production', err)
        opener.assert_not_called()

    def test_sandbox_link_uses_sandbox_token_host(self):
        self.link(environment='sandbox')
        with patch.object(self.mod._OPENER, 'open', self.fake_open()):
            code, _, _ = self.run_main('call', 'ebay', 'GET', 'https://api.sandbox.ebay.com/sell/inventory/v1/inventory_item')
        self.assertEqual(code, 0)
        self.assertEqual(self.token_calls[0].full_url, 'https://api.sandbox.ebay.com/identity/v1/oauth2/token')

    def test_missing_client_secret_cannot_call(self):
        self.link(client_secret='')
        with patch.object(self.mod._OPENER, 'open') as opener:
            code, _, err = self.run_main('call', 'ebay', 'GET', EBAY_INVENTORY)
        self.assertEqual(code, 3)
        self.assertIn('reconnect', err)
        opener.assert_not_called()

    def test_secrets_are_scrubbed_from_output(self):
        echo = ' '.join((EBAY_REFRESH, EBAY_SECRET, EBAY_ACCESS + '1'))
        with patch.object(self.mod._OPENER, 'open', self.fake_open(api_body=echo)):
            code, out, _ = self.run_main('call', 'ebay', 'GET', EBAY_INVENTORY)
        self.assertEqual(code, 0)
        for secret in (EBAY_REFRESH, EBAY_SECRET, EBAY_ACCESS):
            self.assertNotIn(secret, out)

    def test_info_shows_marketplace_without_secrets(self):
        code, out, _ = self.run_main('info', 'ebay')
        self.assertEqual(code, 0)
        info = json.loads(out)
        self.assertEqual(info['marketplace_id'], 'EBAY_GB')
        self.assertEqual(info['environment'], 'production')
        self.assertTrue(info['auto_refresh'])
        for secret in (EBAY_REFRESH, EBAY_SECRET):
            self.assertNotIn(secret, out)


def urllib_parse_qsl(text):
    from urllib.parse import parse_qsl
    return parse_qsl(text, keep_blank_values=True)


if __name__ == '__main__':
    unittest.main()
