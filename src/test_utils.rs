#![cfg(test)]

// The location of the test nsf file
pub const TEST_NSF_PATH: &str = "assets/test.nsf";

/// Load the bytes for the nsf.test
pub fn get_test_nsf_data() -> Vec<u8> {
    std::fs::read(TEST_NSF_PATH).unwrap()
}
