from py.src.FlatCityBuf.ColumnMeta import ColumnMeta as FBColumnMeta
from py.src.FlatCityBuf.ColumnType import ColumnType
import flatbuffers


class ColumnMeta:
    def __init__(
        self,
        name: str,
        type: ColumnType,
        title: str | None = None,
        description: str | None = None,
        precision: int = -1,
        scale: int = -1,
        nullable: bool = True,
        unique: bool = False,
        primary_key: bool = False,
        metadata: str | None = None,
    ):
        self.name = name
        self.type = type
        self.title = title
        self.description = description
        self.precision = precision
        self.scale = scale
        self.nullable = nullable
        self.unique = unique
        self.primary_key = primary_key
        self.metadata = metadata

    def __str__(self):
        return (
            f"ColumnMeta(name={self.name}, type={self.type}, title={self.title}, "
            f"description={self.description}, precision={self.precision}, scale={self.scale}, "
            f"nullable={self.nullable}, unique={self.unique}, primary_key={self.primary_key}, "
            f"metadata={self.metadata})"
        )

    def __repr__(self):
        return self.__str__()

    @classmethod
    def from_byte_buffer(cls, bb: bytes):
        """
        Instantiate ColumnMeta from a FlatBuffers byte buffer.

        Args:
            bb (bytes): The byte buffer containing serialized ColumnMeta data.

        Returns:
            ColumnMeta: An instance of ColumnMeta populated with data from the buffer.
        """
        fb_column_meta = FBColumnMeta.GetRootAsColumnMeta(bb, 0)

        # Extract fields with appropriate decoding and handling
        name = (
            fb_column_meta.Name().decode("utf-8")
            if fb_column_meta.Name()
            else ""
        )
        type_enum = fb_column_meta.Type()
        type = ColumnType.Name(
            type_enum
        )  # Convert enum to string or appropriate type
        title = (
            fb_column_meta.Title().decode("utf-8")
            if fb_column_meta.Title()
            else None
        )
        description = (
            fb_column_meta.Description().decode("utf-8")
            if fb_column_meta.Description()
            else None
        )
        precision = fb_column_meta.Precision()
        scale = fb_column_meta.Scale()
        nullable = fb_column_meta.Nullable()
        unique = fb_column_meta.Unique()
        primary_key = fb_column_meta.PrimaryKey()
        metadata = (
            fb_column_meta.Metadata().decode("utf-8")
            if fb_column_meta.Metadata()
            else None
        )

        return cls(
            name=name,
            type=type,
            title=title,
            description=description,
            precision=precision,
            scale=scale,
            nullable=nullable,
            unique=unique,
            primary_key=primary_key,
            metadata=metadata,
        )
