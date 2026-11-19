# Multilayer reference viewer checks

Run the Output Job from each saved reference Project. Record the integration commit, viewer name and date in the [maintainer check](https://github.com/openreda/reda/issues/390). Attach a screenshot and the affected filename for each discrepancy.

## Four-layer assembly coupon

- [ ] Open `FourLayer-JLCPCB.zip` in the JLCPCB and PCBWay fabrication viewers and the Gerber publisher’s reference viewer. Verify that `.gtl`, `.g2l`, `.g3l` and `.gbl` appear in that physical order.
- [ ] Inspect the signal Via at (6, 9) mm. Its copper lands are present on L1 and L2. The Via at (14, 9) mm has lands on L2 and L4. Both holes run through all four Layers.
- [ ] Check the plated test point at (10, 4) mm and the separate non-plated tooling hole at (2, 2) mm. The plated and non-plated drill files identify spans L1–L4.
- [ ] Load `FourLayer-JLCPCB-BOM.csv` and `FourLayer-JLCPCB-CPL.csv` in the two assembly viewers. R1 belongs on top at (4.25, 6.00) mm and 90°. R2 belongs on the bottom at (17.00, 6.25) mm and 180°. R1 uses the Footprint correction. R2 uses the supplier-field correction.

## Six-layer HDI coupon

- [ ] Open `SixLayerHdi-HDI.zip` in the publisher’s reference Gerber viewer. The six copper plots are in physical order and L3 is the named mixed Layer.
- [ ] Inspect the laser drills L1–L2 and L2–L3 at the same position (6, 9) mm. Check the buried L3–L5 drill at (10, 9) mm and the blind L5–L6 drill at (14, 9) mm.
- [ ] Verify the separate through-plated test point and non-plated tooling hole. Each drill file should appear once, with its own span and plating.
