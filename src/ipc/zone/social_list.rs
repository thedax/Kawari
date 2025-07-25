use binrw::binrw;

use crate::common::{CHAR_NAME_MAX_LENGTH, read_string, write_string};

#[binrw]
#[brw(repr = u8)]
#[derive(Debug, Clone, Copy, Default)]
pub enum SocialListRequestType {
    #[default]
    Party = 0x1,
    Friends = 0x2,
}

#[binrw]
#[derive(Debug, Clone, Default)]
pub struct SocialListRequest {
    #[brw(pad_before = 10)] // empty
    pub request_type: SocialListRequestType,
    #[brw(pad_after = 4)] // empty
    pub count: u8,
}

#[binrw]
#[derive(Debug, Clone, Default)]
pub struct PlayerEntry {
    pub content_id: u64,
    pub unk: [u8; 16],
    pub zone_id: u16,
    pub zone_id1: u16,
    pub unk2: [u8; 8],
    pub online_status_mask: u64,
    pub unk3: [u8; 22],
    pub world_id: u16,
    #[br(count = CHAR_NAME_MAX_LENGTH)]
    #[bw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
    #[br(map = read_string)]
    #[bw(map = write_string)]
    pub name: String,
    #[brw(pad_after = 6)]
    #[br(count = 6)]
    #[bw(pad_size_to = 6)]
    #[br(map = read_string)]
    #[bw(map = write_string)]
    pub fc_tag: String,
}

impl PlayerEntry {
    pub const SIZE: usize = 112;
}

#[binrw]
#[derive(Debug, Clone, Default)]
pub struct SocialList {
    #[brw(pad_before = 12)] // empty
    pub request_type: SocialListRequestType,
    pub sequence: u8,
    #[brw(pad_before = 2)] // empty
    #[br(count = 10)]
    #[bw(pad_size_to = 10 * PlayerEntry::SIZE)]
    pub entries: Vec<PlayerEntry>,
}
