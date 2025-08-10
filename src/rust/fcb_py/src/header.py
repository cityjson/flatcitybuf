from flatbuffers import *

from py.src.FlatCityBuf import (
    Transform,
    GeographicalExtent,
    ReferenceSystem,
)


class HeaderMeta:
    def __init__(
        self,
        transform: Transform,
        columns: List[ColumnMeta],
        features_count: int,
        index_node_size: int,
        geographical_extent: GeographicalExtent,
        reference_system: ReferenceSystem,
        identifier: str,
        reference_date: str,
        title: str,
        poc_contact_name: str,
        poc_contact_type: str,
        poc_role: str,
        poc_phone: str,
        poc_email: str,
        poc_website: str,
        poc_address_thoroughfare_number: str,
        poc_address_thoroughfare_name: str,
        poc_address_locality: str,
        poc_address_postcode: str,
        poc_address_country: str,
        attributes: List[int],
    ):
        self.transform = transform
        self.columns = columns
        self.features_count = features_count
        self.index_node_size = index_node_size
        self.geographical_extent = geographical_extent
        self.reference_system = reference_system
        self.identifier = identifier
        self.reference_date = reference_date
        self.title = title
        self.poc_contact_name = poc_contact_name
        self.poc_contact_type = poc_contact_type
        self.poc_role = poc_role
        self.poc_phone = poc_phone
        self.poc_email = poc_email
        self.poc_website = poc_website
        self.poc_address_thoroughfare_number = poc_address_thoroughfare_number
        self.poc_address_thoroughfare_name = poc_address_thoroughfare_name
        self.poc_address_locality = poc_address_locality
        self.poc_address_postcode = poc_address_postcode
        self.poc_address_country = poc_address_country
        self.attributes = attributes
