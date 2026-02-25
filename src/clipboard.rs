//! This has all the logic regarding the cliboard history
use arboard::ImageData;

/// The kinds of clipboard content that Coco can handle and their contents
#[derive(Debug, Clone)]
pub enum ClipBoardContentType {
    Text(String),
    Image(ImageData<'static>),
}

impl PartialEq for ClipBoardContentType {
    /// Let cliboard items be comparable
    fn eq(&self, other: &Self) -> bool {
        if let Self::Text(a) = self
            && let Self::Text(b) = other
        {
            return a == b;
        } else if let Self::Image(image_data) = self
            && let Self::Image(other_image_data) = other
        {
            return image_data.bytes == other_image_data.bytes;
        }
        false
    }
}
