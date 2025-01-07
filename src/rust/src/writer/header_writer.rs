use crate::error::CityJSONError;
use crate::header_generated::{
    GeographicalExtent, Header, HeaderArgs, ReferenceSystem, ReferenceSystemArgs, Transform, Vector,
};
use cjseq::{CityJSON, Metadata as CJMetadata, Transform as CjTransform};
use flatbuffers::FlatBufferBuilder;

// impl<'a> HeaderWriter<'a> {}
pub struct HeaderWriter<'a> {
    fbb: FlatBufferBuilder<'a>,
    cj: CityJSON,
    header_options: HeaderWriterOptions,
}

pub struct HeaderWriterOptions {
    pub write_index: bool,
    pub header_metadata: HeaderMetadata,
}

pub struct HeaderMetadata {
    pub features_count: u64,
}

impl Default for HeaderWriterOptions {
    fn default() -> Self {
        HeaderWriterOptions {
            write_index: true,
            header_metadata: HeaderMetadata { features_count: 0 },
        }
    }
}

impl<'a> HeaderWriter<'a> {
    pub fn new(cj: CityJSON, header_options: Option<HeaderWriterOptions>) -> HeaderWriter<'a> {
        Self::new_with_options(header_options.unwrap_or_default(), cj)
    }
    pub fn new_with_options(options: HeaderWriterOptions, cj: CityJSON) -> HeaderWriter<'a> {
        let fbb = FlatBufferBuilder::new();

        HeaderWriter {
            fbb,
            cj,
            header_options: options,
        }
    }

    pub fn finish_to_header(mut self) -> Vec<u8> {
        let header = self.create_header();
        self.fbb.finish_size_prefixed(header, None);
        self.fbb.finished_data().to_vec()
    }

    fn create_header(&mut self) -> flatbuffers::WIPOffset<Header<'a>> {
        let metadata = self
            .cj
            .metadata
            .as_ref()
            .ok_or(CityJSONError::MissingField("metadata"))
            .unwrap();
        let reference_system = Self::reference_system(&mut self.fbb, metadata);
        let transform = Self::transform(&self.cj.transform);
        let geographical_extent = metadata
            .geographical_extent
            .as_ref()
            .map(Self::geographical_extent);
        let features_count = self.header_options.header_metadata.features_count;
        let header_args = HeaderArgs {
            version: Some(self.fbb.create_string(&self.cj.version)),
            transform: Some(&transform),
            columns: None,
            features_count,
            geographical_extent: geographical_extent.as_ref(),
            reference_system,
            identifier: metadata
                .identifier
                .as_ref()
                .map(|i| self.fbb.create_string(i)),
            reference_date: metadata
                .reference_date
                .as_ref()
                .map(|r| self.fbb.create_string(r)),
            title: metadata.title.as_ref().map(|t| self.fbb.create_string(t)),
            poc_contact_name: metadata
                .point_of_contact
                .as_ref()
                .map(|poc| self.fbb.create_string(&poc.contact_name)),
            poc_contact_type: metadata.point_of_contact.as_ref().and_then(|poc| {
                poc.contact_type
                    .as_ref()
                    .map(|ct| self.fbb.create_string(ct))
            }),
            poc_role: metadata
                .point_of_contact
                .as_ref()
                .and_then(|poc| poc.role.as_ref().map(|r| self.fbb.create_string(r))),
            poc_phone: metadata
                .point_of_contact
                .as_ref()
                .and_then(|poc| poc.phone.as_ref().map(|p| self.fbb.create_string(p))),
            poc_email: metadata
                .point_of_contact
                .as_ref()
                .map(|poc| self.fbb.create_string(&poc.email_address)),
            poc_website: metadata
                .point_of_contact
                .as_ref()
                .and_then(|poc| poc.website.as_ref().map(|w| self.fbb.create_string(w))),
            poc_address_thoroughfare_number: metadata.point_of_contact.as_ref().and_then(|poc| {
                poc.address
                    .as_ref()
                    .map(|a| self.fbb.create_string(&a.thoroughfare_number.to_string()))
            }),
            poc_address_thoroughfare_name: metadata.point_of_contact.as_ref().map(|poc| {
                self.fbb.create_string(
                    &poc.address
                        .as_ref()
                        .map(|a| a.thoroughfare_name.clone())
                        .unwrap_or_default(),
                )
            }),
            poc_address_locality: metadata.point_of_contact.as_ref().map(|poc| {
                self.fbb.create_string(
                    &poc.address
                        .as_ref()
                        .map(|a| a.locality.clone())
                        .unwrap_or_default(),
                )
            }),
            poc_address_postcode: metadata.point_of_contact.as_ref().map(|poc| {
                self.fbb.create_string(
                    &poc.address
                        .as_ref()
                        .map(|a| a.postal_code.clone())
                        .unwrap_or_default(),
                )
            }),
            poc_address_country: metadata.point_of_contact.as_ref().map(|poc| {
                self.fbb.create_string(
                    &poc.address
                        .as_ref()
                        .map(|a| a.country.clone())
                        .unwrap_or_default(),
                )
            }),
            attributes: None,
        };

        Header::create(&mut self.fbb, &header_args)
    }

    fn geographical_extent(geographical_extent: &[f64; 6]) -> GeographicalExtent {
        let min = Vector::new(
            geographical_extent[0],
            geographical_extent[1],
            geographical_extent[2],
        );
        let max = Vector::new(
            geographical_extent[3],
            geographical_extent[4],
            geographical_extent[5],
        );
        GeographicalExtent::new(&min, &max)
    }

    fn transform(transform: &CjTransform) -> Transform {
        let scale = Vector::new(transform.scale[0], transform.scale[1], transform.scale[2]);
        let translate = Vector::new(
            transform.translate[0],
            transform.translate[1],
            transform.translate[2],
        );
        Transform::new(&scale, &translate)
    }

    fn reference_system(
        fbb: &mut FlatBufferBuilder<'a>,
        metadata: &CJMetadata,
    ) -> Option<flatbuffers::WIPOffset<ReferenceSystem<'a>>> {
        metadata.reference_system.as_ref().map(|ref_sys| {
            let authority = Some(fbb.create_string(&ref_sys.authority));

            let version = ref_sys.version.parse::<i32>().unwrap_or_else(|e| {
                println!("Failed to parse version: {}", e);
                0
            });
            let code = ref_sys.code.parse::<i32>().unwrap_or_else(|e| {
                println!("Failed to parse code: {}", e);
                0
            });

            let code_string = None; // TODO: implement code_string

            ReferenceSystem::create(
                fbb,
                &ReferenceSystemArgs {
                    authority,
                    version,
                    code,
                    code_string,
                },
            )
        })
    }
}
