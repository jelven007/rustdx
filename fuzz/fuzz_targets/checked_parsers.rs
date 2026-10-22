#![no_main]
use libfuzzer_sys::fuzz_target;
use rustdx_complete::{
    bytes_helper::{into_arr2, into_arr4},
    file::{
        day::Day,
        gbbq::{Gbbq, Gbbqs, try_parse},
        lc::Lc,
    },
    tcp::{
        Tdx,
        stock::{Kline, KlineRequest, SecurityQuotes, decode_index_bars, decode_stock_bars},
    },
};

fuzz_target!(|data: &[u8]| {
    if data.len() > 65536 {
        return;
    }
    let _ = Day::try_from_bytes("sz000001", data);
    let _ = Day::from_bytes_into_vec("sz000001", data);
    let _ = Lc::try_from_bytes(1, data);
    let _ = Lc::from_bytes_into_vec(1, data);
    let _ = Gbbq::try_from_chunk(data);
    let mut encrypted = data.to_vec();
    let _ = try_parse(&mut encrypted);
    if let Ok(mut source) = Gbbqs::from_bytes(data.to_vec()) {
        let _ = source.try_to_vec();
    }
    let _ = into_arr2(data, usize::MAX);
    let _ = into_arr4(data, usize::MAX);
    let mut quotes = SecurityQuotes::default();
    let _ = quotes.try_parse(data.to_vec());
    let mut bars = Kline::new(0, "000001", 9, 0, 800);
    let _ = bars.try_parse(data.to_vec());
    for category in [0, 9] {
        let request = KlineRequest::new(0, "000001", category, 0, 800)
            .expect("constant checked request");
        let _ = decode_stock_bars(&request, data);
        let _ = decode_index_bars(&request, data);
    }
});
