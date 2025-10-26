#![cfg(test)]

// The location of the test nsf file
pub const TEST_NSF_PATH: &str = "assets/test.nsf";

/// The location of the test m3u file
pub const TEST_M3U_PATH: &str = "assets/test.m3u";

/// Load the bytes for the nsf.test
pub fn get_test_nsf_data() -> Vec<u8> {
    std::fs::read(TEST_NSF_PATH).unwrap()
}

/// Load the bytes for the m3u.test
pub fn get_test_m3u_data() -> Vec<u8> {
    std::fs::read(TEST_M3U_PATH).unwrap()
}
