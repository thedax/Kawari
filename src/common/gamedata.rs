use std::path::PathBuf;

use icarus::Action::ActionSheet;
use icarus::Aetheryte::AetheryteSheet;
use icarus::ClassJob::ClassJobSheet;
use icarus::ContentFinderCondition::ContentFinderConditionSheet;
use icarus::EquipSlotCategory::EquipSlotCategorySheet;
use icarus::GilShopItem::GilShopItemSheet;
use icarus::InstanceContent::InstanceContentSheet;
use icarus::PlaceName::PlaceNameSheet;
use icarus::TerritoryType::TerritoryTypeSheet;
use icarus::WeatherRate::WeatherRateSheet;
use icarus::World::WorldSheet;
use icarus::{Tribe::TribeSheet, Warp::WarpSheet};
use physis::common::{Language, Platform};
use physis::exd::{EXD, ExcelRowKind};
use physis::exh::EXH;
use physis::resource::{
    Resource, ResourceResolver, SqPackResource, UnpackedResource, read_excel_sheet,
    read_excel_sheet_header,
};

use crate::{common::Attributes, config::get_config};

use super::timestamp_secs;

/// Convenient methods built on top of Physis to access data relevant to the server
pub struct GameData {
    pub resource: ResourceResolver,
    pub item_exh: EXH,
    pub item_pages: Vec<EXD>,
    pub classjob_exp_indexes: Vec<i8>,
}

impl Default for GameData {
    fn default() -> Self {
        Self::new()
    }
}

/// Struct detailing various information about an item, pulled from the Items sheet.
#[derive(Default, Clone)]
pub struct ItemInfo {
    /// The item's textual name.
    pub name: String,
    /// The item's id number.
    pub id: u32,
    /// The item's price, when sold by an NPC.
    pub price_mid: u32,
    /// The item's price, when sold to an NPC by the player.
    pub price_low: u32,
    /// The item's equip category.
    pub equip_category: u8,
    /// The item's primary model id.
    pub primary_model_id: u64,
    /// The item's max stack size.
    pub stack_size: u32,
    /// The item's item level.
    pub item_level: u16,
}

#[derive(Debug)]
pub enum ItemInfoQuery {
    ById(u32),
    ByName(String),
}

impl GameData {
    pub fn new() -> Self {
        let config = get_config();

        // setup resolvers
        let sqpack_resource = SqPackResourceSpy::from(
            SqPackResource::from_existing(Platform::Win32, &config.filesystem.game_path),
            &config.filesystem.unpack_path,
        );

        if sqpack_resource.sqpack_resource.repositories.is_empty() {
            tracing::warn!(
                "You have an empty game directory ({:?}). This may be a configuration issue, you may want to read the usage documentation.",
                config.filesystem.game_path
            );
        }

        let mut resource_resolver = ResourceResolver::new();
        for path in config.filesystem.additional_search_paths {
            let unpacked_resource = UnpackedResource::from_existing(&path);
            resource_resolver.add_source(Box::new(unpacked_resource));
        }
        resource_resolver.add_source(Box::new(sqpack_resource));

        let mut item_pages = Vec::new();

        let item_exh = read_excel_sheet_header(&mut resource_resolver, "Item")
            .expect("Failed to read Item EXH, does the file exist?");
        for (i, _) in item_exh.pages.iter().enumerate() {
            item_pages.push(
                read_excel_sheet(
                    &mut resource_resolver,
                    "Item",
                    &item_exh,
                    Language::English,
                    i,
                )
                .expect("Failed to read Item EXD, does the file exist?"),
            );
        }

        let mut classjob_exp_indexes = Vec::new();

        let sheet = ClassJobSheet::read_from(&mut resource_resolver, Language::English)
            .expect("Failed to read ClassJobSheet, does the Excel files exist?");
        // TODO: ids are hardcoded until we have API in Icarus to do this
        for i in 0..43 {
            let row = sheet.get_row(i).unwrap();

            classjob_exp_indexes.push(*row.ExpArrayIndex().into_i8().unwrap());
        }

        Self {
            resource: resource_resolver,
            item_exh,
            item_pages,
            classjob_exp_indexes,
        }
    }

    /// Gets the world name from an id into the World Excel sheet.
    pub fn get_world_name(&mut self, world_id: u16) -> Option<String> {
        let sheet = WorldSheet::read_from(&mut self.resource, Language::None)?;
        let row = sheet.get_row(world_id as u32)?;

        row.Name().into_string().cloned()
    }

    /// Gets the starting city-state from a given class/job id.
    pub fn get_citystate(&mut self, classjob_id: u16) -> Option<u8> {
        let sheet = ClassJobSheet::read_from(&mut self.resource, Language::English)?;
        let row = sheet.get_row(classjob_id as u32)?;

        row.StartingTown().into_u8().copied()
    }

    pub fn get_racial_base_attributes(&mut self, tribe_id: u8) -> Option<Attributes> {
        // The Tribe Excel sheet only has deltas (e.g. 2 or -2) which are applied to a base 20 number... from somewhere
        let base_stat = 20;

        let sheet = TribeSheet::read_from(&mut self.resource, Language::English)?;
        let row = sheet.get_row(tribe_id as u32)?;

        Some(Attributes {
            strength: (base_stat + row.STR().into_i8()?) as u32,
            dexterity: (base_stat + row.DEX().into_i8()?) as u32,
            vitality: (base_stat + row.VIT().into_i8()?) as u32,
            intelligence: (base_stat + row.INT().into_i8()?) as u32,
            mind: (base_stat + row.MND().into_i8()?) as u32,
        })
    }

    /// Gets various information from the Item sheet.
    pub fn get_item_info(&mut self, query: ItemInfoQuery) -> Option<ItemInfo> {
        let mut result = None;
        'outer: for page in &self.item_pages {
            match query {
                ItemInfoQuery::ById(ref query_item_id) => {
                    if let Some(row) = page.get_row(*query_item_id) {
                        let ExcelRowKind::SingleRow(item_row) = row else {
                            panic!("Expected a single row!");
                        };
                        result = Some((item_row, query_item_id));
                        break 'outer;
                    }
                }

                ItemInfoQuery::ByName(ref query_item_name) => {
                    for row in &page.rows {
                        let ExcelRowKind::SingleRow(single_row) = &row.kind else {
                            panic!("Expected a single row!");
                        };

                        let physis::exd::ColumnData::String(item_name) = &single_row.columns[9]
                        else {
                            panic!("Unexpected type!");
                        };

                        if !item_name
                            .to_lowercase()
                            .contains(&query_item_name.to_lowercase())
                        {
                            continue;
                        }

                        result = Some((single_row.clone(), &row.row_id));
                        break 'outer;
                    }
                }
            }
        }

        if let Some((matched_row, item_id)) = result {
            let physis::exd::ColumnData::String(name) = &matched_row.columns[9] else {
                panic!("Unexpected type!");
            };

            let physis::exd::ColumnData::UInt16(item_level) = &matched_row.columns[11] else {
                panic!("Unexpected type!");
            };

            let physis::exd::ColumnData::UInt8(equip_category) = &matched_row.columns[17] else {
                panic!("Unexpected type!");
            };

            let physis::exd::ColumnData::UInt32(stack_size) = &matched_row.columns[20] else {
                panic!("Unexpected type!");
            };

            let physis::exd::ColumnData::UInt32(price_mid) = &matched_row.columns[25] else {
                panic!("Unexpected type!");
            };

            let physis::exd::ColumnData::UInt32(price_low) = &matched_row.columns[26] else {
                panic!("Unexpected type!");
            };

            let physis::exd::ColumnData::UInt64(primary_model_id) = &matched_row.columns[47] else {
                panic!("Unexpected type!");
            };

            let item_info = ItemInfo {
                id: *item_id,
                name: name.to_string(),
                price_mid: *price_mid,
                price_low: *price_low,
                equip_category: *equip_category,
                primary_model_id: *primary_model_id,
                stack_size: *stack_size,
                item_level: *item_level,
            };

            return Some(item_info);
        }

        None
    }

    /// Gets the primary model ID for a given item ID
    pub fn get_primary_model_id(&mut self, item_id: u32) -> Option<u64> {
        if let Some(item_info) = self.get_item_info(ItemInfoQuery::ById(item_id)) {
            return Some(item_info.primary_model_id);
        }

        None
    }

    /// Returns the pop range object id that's associated with the warp id
    pub fn get_warp(&mut self, warp_id: u32) -> Option<(u32, u16)> {
        let sheet = WarpSheet::read_from(&mut self.resource, Language::English)?;
        let row = sheet.get_row(warp_id)?;

        let pop_range_id = row.PopRange().into_u32()?;
        let zone_id = row.TerritoryType().into_u16()?;

        Some((*pop_range_id, *zone_id))
    }

    pub fn get_aetheryte(&mut self, aetheryte_id: u32) -> Option<(u32, u16)> {
        let sheet = AetheryteSheet::read_from(&mut self.resource, Language::English)?;
        let row = sheet.get_row(aetheryte_id)?;

        // TODO: just look in the level sheet?
        let pop_range_id = row.Level()[0].into_u32()?;
        let zone_id = row.Territory().into_u16()?;

        Some((*pop_range_id, *zone_id))
    }

    // Retrieves a zone's internal name, place name or parent region name.
    pub fn get_territory_name(&mut self, zone_id: u32, which: TerritoryNameKind) -> Option<String> {
        let sheet = TerritoryTypeSheet::read_from(&mut self.resource, Language::None)?;
        let row = sheet.get_row(zone_id)?;

        let offset = match which {
            TerritoryNameKind::Internal => {
                return row.Name().into_string().cloned();
            }
            TerritoryNameKind::Region => row.PlaceNameRegion().into_u16()?,
            TerritoryNameKind::Place => row.PlaceName().into_u16()?,
        };

        let sheet = PlaceNameSheet::read_from(&mut self.resource, Language::English)?;
        let row = sheet.get_row(*offset as u32)?;

        let value = row.Name().into_string()?;

        Some(value.clone())
    }

    /// Turn an equip slot category id into a slot for the equipped inventory
    pub fn get_equipslot_category(&mut self, equipslot_id: u8) -> Option<u16> {
        let sheet = EquipSlotCategorySheet::read_from(&mut self.resource, Language::None)?;
        let row = sheet.get_row(equipslot_id as u32)?;

        let main_hand = row.MainHand().into_i8()?;
        if *main_hand == 1 {
            return Some(0);
        }

        let off_hand = row.OffHand().into_i8()?;
        if *off_hand == 1 {
            return Some(1);
        }

        let head = row.Head().into_i8()?;
        if *head == 1 {
            return Some(2);
        }

        let body = row.Body().into_i8()?;
        if *body == 1 {
            return Some(3);
        }

        let gloves = row.Gloves().into_i8()?;
        if *gloves == 1 {
            return Some(4);
        }

        let legs = row.Legs().into_i8()?;
        if *legs == 1 {
            return Some(6);
        }

        let feet = row.Feet().into_i8()?;
        if *feet == 1 {
            return Some(7);
        }

        let ears = row.Ears().into_i8()?;
        if *ears == 1 {
            return Some(8);
        }

        let neck = row.Neck().into_i8()?;
        if *neck == 1 {
            return Some(9);
        }

        let wrists = row.Wrists().into_i8()?;
        if *wrists == 1 {
            return Some(10);
        }

        let right_finger = row.FingerR().into_i8()?;
        if *right_finger == 1 {
            return Some(11);
        }

        let left_finger = row.FingerL().into_i8()?;
        if *left_finger == 1 {
            return Some(12);
        }

        let soul_crystal = row.SoulCrystal().into_i8()?;
        if *soul_crystal == 1 {
            return Some(13);
        }

        None
    }

    pub fn get_casttime(&mut self, action_id: u32) -> Option<u16> {
        let sheet = ActionSheet::read_from(&mut self.resource, Language::English)?;
        let row = sheet.get_row(action_id)?;

        row.Cast100ms().into_u16().copied()
    }

    /// Calculates the current weather at the current time
    // TODO: instead allow targetting a specific time to calculate forcecasts
    pub fn get_weather_rate(&mut self, weather_rate_id: u32) -> Option<i32> {
        let sheet = WeatherRateSheet::read_from(&mut self.resource, Language::None)?;
        let row = sheet.get_row(weather_rate_id)?;

        let target = Self::calculate_target();
        let weather_and_rates: Vec<(i32, i32)> = row
            .Weather()
            .iter()
            .cloned()
            .zip(row.Rate())
            .map(|(x, y)| (*x.into_i32().unwrap(), *y.into_u8().unwrap() as i32))
            .collect();

        Some(
            weather_and_rates
                .iter()
                .filter(|(_, rate)| target < *rate)
                .take(1)
                .collect::<Vec<&(i32, i32)>>()
                .first()?
                .0,
        )
    }

    /// Calculate target window for weather calculations
    fn calculate_target() -> i32 {
        // Based off of https://github.com/Rogueadyn/SaintCoinach/blob/master/SaintCoinach/Xiv/WeatherRate.cs
        // TODO: this isn't correct still and doesn't seem to match up with the retail server

        let real_to_eorzean_factor = (60.0 * 24.0) / 70.0;
        let unix = (timestamp_secs() as f32 / real_to_eorzean_factor) as u64;
        // Get Eorzea hour for weather start
        let bell = unix / 175;
        // Do the magic 'cause for calculations 16:00 is 0, 00:00 is 8 and 08:00 is 16
        let increment = ((bell + 8 - (bell % 8)) as u32) % 24;

        // Take Eorzea days since unix epoch
        let total_days = (unix / 4200) as u32;

        let calc_base = (total_days * 0x64) + increment;

        let step1 = (calc_base << 0xB) ^ calc_base;
        let step2 = (step1 >> 8) ^ step1;

        (step2 % 0x64) as i32
    }

    /// Gets the current weather for the given zone id
    pub fn get_weather(&mut self, zone_id: u32) -> Option<i32> {
        let sheet = TerritoryTypeSheet::read_from(&mut self.resource, Language::None)?;
        let row = sheet.get_row(zone_id)?;

        let weather_rate_id = row.WeatherRate().into_u8()?;

        self.get_weather_rate(*weather_rate_id as u32)
    }

    /// Gets the array index used in EXP & levels.
    pub fn get_exp_array_index(&self, classjob_id: u16) -> Option<i8> {
        self.classjob_exp_indexes.get(classjob_id as usize).copied()
    }

    /// Gets the item and its cost from the specified shop.
    pub fn get_gilshop_item(&mut self, gilshop_id: u32, index: u16) -> Option<ItemInfo> {
        let sheet = GilShopItemSheet::read_from(&mut self.resource, Language::None)?;
        let row = sheet.get_subrow(gilshop_id, index)?;
        let item_id = row.Item().into_i32()?;

        self.get_item_info(ItemInfoQuery::ById(*item_id as u32))
    }

    /// Gets the zone id for the given InstanceContent.
    pub fn find_zone_for_content(&mut self, content_id: u16) -> Option<u16> {
        let instance_content_sheet =
            InstanceContentSheet::read_from(&mut self.resource, Language::None).unwrap();
        let instance_content_row = instance_content_sheet.get_row(content_id as u32)?;

        let content_finder_row_id = instance_content_row.ContentFinderCondition().into_u16()?;
        let content_finder_sheet =
            ContentFinderConditionSheet::read_from(&mut self.resource, Language::English).unwrap();
        let content_finder_row = content_finder_sheet.get_row(*content_finder_row_id as u32)?;

        content_finder_row.TerritoryType().into_u16().copied()
    }
}

// Simple enum for GameData::get_territory_name
pub enum TerritoryNameKind {
    Internal,
    Region,
    Place,
}

/// Wrapper around SqPackResource to let us spy when it reads files
struct SqPackResourceSpy {
    sqpack_resource: SqPackResource,
    output_directory: String,
}

impl SqPackResourceSpy {
    pub fn from(sqpack_resource: SqPackResource, output_directory: &str) -> Self {
        Self {
            sqpack_resource,
            output_directory: output_directory.to_string(),
        }
    }
}

impl Resource for SqPackResourceSpy {
    fn read(&mut self, path: &str) -> Option<physis::ByteBuffer> {
        if let Some(buffer) = self.sqpack_resource.read(path) {
            if !self.output_directory.is_empty() {
                let mut new_path = PathBuf::from(&self.output_directory);
                new_path.push(path.to_lowercase());

                if !std::fs::exists(&new_path).unwrap_or_default() {
                    // create directory if it doesn't exist'
                    let parent_directory = new_path.parent().unwrap();
                    if !std::fs::exists(parent_directory).unwrap_or_default() {
                        std::fs::create_dir_all(parent_directory)
                            .expect("Couldn't create directory for extraction?!");
                    }

                    std::fs::write(new_path, &buffer).expect("Couldn't extract file!!");
                }
            }

            return Some(buffer);
        }

        None
    }

    fn exists(&mut self, path: &str) -> bool {
        self.sqpack_resource.exists(path)
    }
}
