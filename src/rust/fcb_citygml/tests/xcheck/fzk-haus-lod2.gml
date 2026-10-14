<?xml version="1.0" encoding="utf-8"?><!-- IFC to CityGML by IFCExplorer KIT --><!-- CityGML to Sketchup by  Sketchup CityGML Plugin FH GelsenKirchen --><!--CityGML Dataset produced with CityGML Export Plugin for Sketchup by GeoRES --><!--http://www.geores.de --><!-- Edited Manually in Oxygen 8.2 --><!-- Modified by GMLOffset.xslt at Mon Dec 6 2010 --><!-- Version 2 Building located in the area of KIT Campus North)--><!-- Modified by GMLOffset.xslt at Wed Dec 8 2010 --><!-- Modified by GMLOffset.xslt at Wed Mar 29 2017 --><core:CityModel xsi:schemaLocation="http://www.opengis.net/citygml/2.0 http://schemas.opengis.net/citygml/2.0/cityGMLBase.xsd  http://www.opengis.net/citygml/appearance/2.0 http://schemas.opengis.net/citygml/appearance/2.0/appearance.xsd http://www.opengis.net/citygml/building/2.0 http://schemas.opengis.net/citygml/building/2.0/building.xsd http://www.opengis.net/citygml/generics/2.0 http://schemas.opengis.net/citygml/generics/2.0/generics.xsd" xmlns:core="http://www.opengis.net/citygml/2.0" xmlns="http://www.opengis.net/citygml/profiles/base/2.0" xmlns:bldg="http://www.opengis.net/citygml/building/2.0" xmlns:gen="http://www.opengis.net/citygml/generics/2.0" xmlns:grp="http://www.opengis.net/citygml/cityobjectgroup/2.0" xmlns:app="http://www.opengis.net/citygml/appearance/2.0" xmlns:gml="http://www.opengis.net/gml" xmlns:xAL="urn:oasis:names:tc:ciq:xsdschema:xAL:2.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
    <!-- Manually edited by KHH 23.01.2017, CityGML 2.0, Address added, Codespaces added -->
    <gml:name>AC14-FZK-Haus</gml:name>
    <gml:boundedBy>
        <gml:Envelope srsDimension="3" srsName="urn:adv:crs:ETRS89_UTM32*DE_DHHN92_NH">
            <gml:lowerCorner srsDimension="3">457842 5439083 111.8 </gml:lowerCorner>
            <gml:upperCorner srsDimension="3">457854 5439093 118.317669 </gml:upperCorner>
        </gml:Envelope>
    </gml:boundedBy> 
    <core:cityObjectMember>
        <bldg:Building gml:id="UUID_d281adfc-4901-0f52-540b-4cc1a9325f82">
            <gml:description>FZK-Haus (Forschungszentrum Karlsruhe, now KIT), created by Karl-Heinz
                Haefele </gml:description>
            <gml:name>AC14-FZK-Haus</gml:name>
            <core:creationDate>2017-01-23</core:creationDate>
            <core:relativeToTerrain>entirelyAboveTerrain</core:relativeToTerrain>
            <gen:measureAttribute name="GrossPlannedArea">
                <gen:value uom="m2">120.00</gen:value>
            </gen:measureAttribute>
            <gen:stringAttribute name="ConstructionMethod">
                <gen:value>New Building</gen:value>
            </gen:stringAttribute>
            <gen:stringAttribute name="IsLandmarked">
                <gen:value>NO</gen:value>
            </gen:stringAttribute>
            <bldg:class codeSpace="http://www.sig3d.org/codelists/citygml/2.0/building/2.0/_AbstractBuilding_class.xml">1000</bldg:class>
            <bldg:function codeSpace="http://www.sig3d.org/codelists/citygml/2.0/building/2.0/_AbstractBuilding_function.xml">1000</bldg:function>
            <bldg:usage codeSpace="http://www.sig3d.org/codelists/citygml/2.0/building/2.0/_AbstractBuilding_usage.xml">1000</bldg:usage>
            <bldg:yearOfConstruction>2020</bldg:yearOfConstruction>
            <bldg:roofType codeSpace="http://www.sig3d.org/codelists/citygml/2.0/building/2.0/_AbstractBuilding_roofType.xml">1030</bldg:roofType>
            <bldg:measuredHeight uom="m">6.52</bldg:measuredHeight>
            <bldg:storeysAboveGround>2</bldg:storeysAboveGround>
            <bldg:storeysBelowGround>0</bldg:storeysBelowGround>
            <bldg:lod2Solid>
                <gml:Solid>
                    <gml:exterior>
                        <gml:CompositeSurface>
                            <!--Outer Wall 1 (West)  -->
                            <gml:surfaceMember xlink:href="#PolyID7350_878_759628_120742"> </gml:surfaceMember>
                            <!--Outer Wall 1 (West)  -->
                            <!--Outer Wall 2 (South)  -->
                            <gml:surfaceMember xlink:href="#PolyID7351_1722_416019_316876" />
                            <!--Outer Wall 2 (South)  -->
                            <!--Outer Wall 3 (East)  -->
                            <gml:surfaceMember xlink:href="#PolyID7352_230_209861_355851" />
                            <!--Outer Wall 3 (East)  -->
                            <!--Roof 1 (North)  -->
                            <gml:surfaceMember xlink:href="#PolyID7353_166_774155_320806" />
                            <!--Roof 1 (North)  -->
                            <!--Outer Wall 4 (North)  -->
                            <gml:surfaceMember xlink:href="#PolyID7354_1362_450904_410226" />
                            <!--Outer Wall 2 (North)  -->
                            <!--Roof 2 (South)  -->
                            <gml:surfaceMember xlink:href="#PolyID7355_537_416207_260034" />
                            <!--Roof 2 (South)  -->
                            <!--Base Surface  -->
                            <gml:surfaceMember xlink:href="#PolyID7356_612_880782_415367" />
                            <!--Base Surface  -->
                        </gml:CompositeSurface>
                    </gml:exterior>
                </gml:Solid>
            </bldg:lod2Solid>
            <bldg:boundedBy>
                <bldg:WallSurface gml:id="GML_5856d7ad-5e34-498a-817b-9544bfbb1475">
                    <gml:name>Outer Wall 1 (West)</gml:name>
                    <bldg:lod2MultiSurface>
                        <gml:MultiSurface>
                            <gml:surfaceMember>
                                <gml:Polygon gml:id="PolyID7350_878_759628_120742">
                                    <gml:exterior>
                                        <gml:LinearRing gml:id="PolyID7350_878_759628_120742_0">
                                            <gml:pos>457842 5439088 118.317691453624 </gml:pos>
                                            <gml:pos>457842 5439093 115.430940107676 </gml:pos>
                                            <gml:pos>457842 5439093 111.8 </gml:pos>
                                            <gml:pos>457842 5439083 111.8 </gml:pos>
                                            <gml:pos>457842 5439083 115.430940107676 </gml:pos>
                                            <gml:pos>457842 5439088 118.317691453624 </gml:pos>
                                        </gml:LinearRing>
                                    </gml:exterior>
                                </gml:Polygon>
                            </gml:surfaceMember>
                        </gml:MultiSurface>
                    </bldg:lod2MultiSurface>
                </bldg:WallSurface>
            </bldg:boundedBy>
            <bldg:boundedBy>
                <bldg:WallSurface gml:id="GML_d38cf762-c29d-4491-88c9-bdc89e141978">
                    <gml:name>Outer Wall 2 (South)</gml:name>
                    <bldg:lod2MultiSurface>
                        <gml:MultiSurface>
                            <gml:surfaceMember>
                                <gml:Polygon gml:id="PolyID7351_1722_416019_316876">
                                    <gml:exterior>
                                        <gml:LinearRing gml:id="PolyID7351_1722_416019_316876_0">
                                            <gml:pos>457854 5439083 115.430940107676 </gml:pos>
                                            <gml:pos>457842 5439083 115.430940107676 </gml:pos>
                                            <gml:pos>457842 5439083 111.8 </gml:pos>
                                            <gml:pos>457854 5439083 111.8 </gml:pos>
                                            <gml:pos>457854 5439083 115.430940107676 </gml:pos>
                                        </gml:LinearRing>
                                    </gml:exterior>
                                </gml:Polygon>
                            </gml:surfaceMember>
                        </gml:MultiSurface>
                    </bldg:lod2MultiSurface>
                </bldg:WallSurface>
            </bldg:boundedBy>
            <bldg:boundedBy>
                <bldg:WallSurface gml:id="GML_8e5db638-e46a-4739-a98a-2fc2d39c9069">
                    <gml:name>Outer Wall 3 (East)</gml:name>
                    <bldg:lod2MultiSurface>
                        <gml:MultiSurface>
                            <gml:surfaceMember>
                                <gml:Polygon gml:id="PolyID7352_230_209861_355851">
                                    <gml:exterior>
                                        <gml:LinearRing gml:id="PolyID7352_230_209861_355851_0">
                                            <gml:pos>457854 5439088 118.317691453624 </gml:pos>
                                            <gml:pos>457854 5439083 115.430940107676 </gml:pos>
                                            <gml:pos>457854 5439083 111.8 </gml:pos>
                                            <gml:pos>457854 5439093 111.8 </gml:pos>
                                            <gml:pos>457854 5439093 115.430940107676 </gml:pos>
                                            <gml:pos>457854 5439088 118.317691453624 </gml:pos>
                                        </gml:LinearRing>
                                    </gml:exterior>
                                </gml:Polygon>
                            </gml:surfaceMember>
                        </gml:MultiSurface>
                    </bldg:lod2MultiSurface>
                </bldg:WallSurface>
            </bldg:boundedBy>
            <bldg:boundedBy>
                <bldg:RoofSurface gml:id="GML_875d470b-32b4-4985-a4c8-0f02caa342a2">
                    <gml:name>Roof 1 (North)</gml:name>
                    <bldg:lod2MultiSurface>
                        <gml:MultiSurface>
                            <gml:surfaceMember>
                                <gml:Polygon gml:id="PolyID7353_166_774155_320806">
                                    <gml:exterior>
                                        <gml:LinearRing gml:id="PolyID7353_166_774155_320806_0">
                                            <gml:pos>457842 5439088 118.317691453624 </gml:pos>
                                            <gml:pos>457854 5439088 118.317691453624 </gml:pos>
                                            <gml:pos>457854 5439093 115.430940107676 </gml:pos>
                                            <gml:pos>457842 5439093 115.430940107676 </gml:pos>
                                            <gml:pos>457842 5439088 118.317691453624 </gml:pos>
                                        </gml:LinearRing>
                                    </gml:exterior>
                                </gml:Polygon>
                            </gml:surfaceMember>
                        </gml:MultiSurface>
                    </bldg:lod2MultiSurface>
                </bldg:RoofSurface>
            </bldg:boundedBy>
            <bldg:boundedBy>
                <bldg:WallSurface gml:id="GML_0f30f604-e70d-4dfe-ba35-853bc69609cc">
                    <gml:name>Outer Wall 4 (North)</gml:name>
                    <bldg:lod2MultiSurface>
                        <gml:MultiSurface>
                            <gml:surfaceMember>
                                <gml:Polygon gml:id="PolyID7354_1362_450904_410226">
                                    <gml:exterior>
                                        <gml:LinearRing gml:id="PolyID7354_1362_450904_410226_0">
                                            <gml:pos>457842 5439093 115.430940107676 </gml:pos>
                                            <gml:pos>457854 5439093 115.430940107676 </gml:pos>
                                            <gml:pos>457854 5439093 111.8 </gml:pos>
                                            <gml:pos>457842 5439093 111.8 </gml:pos>
                                            <gml:pos>457842 5439093 115.430940107676 </gml:pos>
                                        </gml:LinearRing>
                                    </gml:exterior>
                                </gml:Polygon>
                            </gml:surfaceMember>
                        </gml:MultiSurface>
                    </bldg:lod2MultiSurface>
                </bldg:WallSurface>
            </bldg:boundedBy>
            <bldg:boundedBy>
                <bldg:RoofSurface gml:id="GML_eeb6796a-e261-4d3b-a6f2-475940cca80a">
                    <gml:name>Roof 2 (South)</gml:name>
                    <bldg:lod2MultiSurface>
                        <gml:MultiSurface>
                            <gml:surfaceMember>
                                <gml:Polygon gml:id="PolyID7355_537_416207_260034">
                                    <gml:exterior>
                                        <gml:LinearRing gml:id="PolyID7355_537_416207_260034_0">
                                            <gml:pos>457854 5439083 115.430940107676 </gml:pos>
                                            <gml:pos>457854 5439088 118.317691453624 </gml:pos>
                                            <gml:pos>457842 5439088 118.317691453624 </gml:pos>
                                            <gml:pos>457842 5439083 115.430940107676 </gml:pos>
                                            <gml:pos>457854 5439083 115.430940107676 </gml:pos>
                                        </gml:LinearRing>
                                    </gml:exterior>
                                </gml:Polygon>
                            </gml:surfaceMember>
                        </gml:MultiSurface>
                    </bldg:lod2MultiSurface>
                </bldg:RoofSurface>
            </bldg:boundedBy>
            <bldg:boundedBy>
                <bldg:GroundSurface gml:id="GML_257a8dde-8194-4ca3-b581-abd591dcd6a3">
                    <gml:description>Bodenplatte</gml:description>
                    <gml:name>Base Surface</gml:name>
                    <bldg:lod2MultiSurface>
                        <gml:MultiSurface>
                            <gml:surfaceMember>
                                <gml:Polygon gml:id="PolyID7356_612_880782_415367">
                                    <gml:exterior>
                                        <gml:LinearRing gml:id="PolyID7356_612_880782_415367_0">
                                            <gml:pos>457854 5439083 111.8 </gml:pos>
                                            <gml:pos>457842 5439083 111.8 </gml:pos>
                                            <gml:pos>457842 5439093 111.8 </gml:pos>
                                            <gml:pos>457854 5439093 111.8 </gml:pos>
                                            <gml:pos>457854 5439083 111.8 </gml:pos>
                                        </gml:LinearRing>
                                    </gml:exterior>
                                </gml:Polygon>
                            </gml:surfaceMember>
                        </gml:MultiSurface>
                    </bldg:lod2MultiSurface>
                </bldg:GroundSurface>
            </bldg:boundedBy>
            <bldg:address>
                <core:Address>
                    <core:xalAddress>
                        <xAL:AddressDetails>
                            <xAL:Locality Type="Town">
                                <xAL:LocalityName>Eggenstein-Leopoldshafen</xAL:LocalityName>
                                <xAL:Thoroughfare Type="Street">
                                    <xAL:ThoroughfareNumber>4711</xAL:ThoroughfareNumber>
                                    <xAL:ThoroughfareName>Spöcker Straße</xAL:ThoroughfareName>
                                </xAL:Thoroughfare>
                                <xAL:PostalCode>
                                    <xAL:PostalCodeNumber>76344</xAL:PostalCodeNumber>
                                </xAL:PostalCode>
                            </xAL:Locality>
                        </xAL:AddressDetails>
                    </core:xalAddress>
                </core:Address>
            </bldg:address>
        </bldg:Building>
    </core:cityObjectMember>
</core:CityModel>