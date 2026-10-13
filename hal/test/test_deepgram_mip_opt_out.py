"""Every Deepgram connect opts out of the Model Improvement Program (training on our audio)."""

from hal.drivers.voice.stt.deepgram import _build_flux_listen_params, _build_nova_listen_params


def test_flux_connect_opts_out_of_training():
    params = _build_flux_listen_params(model="flux-general-en", sample_rate=16000)
    assert params["mip_opt_out"] == "true"


def test_nova_connect_opts_out_of_training():
    params = _build_nova_listen_params(
        model="nova-3", channels=1, sample_rate=16000, language="en", keywords=["lamp:2"],
    )
    assert params["mip_opt_out"] == "true"
