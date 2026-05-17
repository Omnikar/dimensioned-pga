use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();

    generate_ga_impls(&out_dir).unwrap();
}

const INPUT_DIR: &str = "src/pga";
fn get_input(name: &str) -> std::io::Result<String> {
    std::fs::read_to_string(Path::new(INPUT_DIR).join(name))
}

type GaStructs<'a> = HashMap<&'a str, Vec<u8>>;

#[derive(Clone)]
struct GaProduct<'a> {
    name: &'a str,
    func_name: &'a str,
    out_type: &'a str,
    overload: Option<&'a str>,
    // For each blade of the output, what elementary product terms contribute?
    terms: [Vec<(bool, u8, u8)>; 16],
}

fn blade_name(bitmask: u8) -> String {
    format!(
        "e{}",
        "0123"
            .chars()
            .enumerate()
            .filter_map(|(i, c)| ((bitmask >> i) & 1 == 1).then_some(c))
            .collect::<String>()
    )
}

fn generate_ga_impls(out_dir: impl AsRef<Path>) -> Result<()> {
    println!("cargo::rerun-if-changed={INPUT_DIR}");

    let out_path = out_dir.as_ref().join("ga_impls.rs");
    let mut out = File::create(out_path)?;

    let structs_text = get_input("structs.txt")?;
    let structs: GaStructs = generate_ga_structs(&mut out, &structs_text)?;

    let products_text = get_input("products.txt")?;
    generate_ga_products(&mut out, &structs, &products_text)?;

    generate_blade_constants(&mut out, &structs)?;

    Ok(())
}

fn generate_ga_structs<'a>(out: &mut File, structs_text: &'a str) -> Result<GaStructs<'a>> {
    let mut structs: GaStructs = structs_text
        .lines()
        .map(|ln| {
            let (name, blades_s) = ln.split_once(": ").unwrap();
            let blades = blades_s
                .split(", ")
                .map(|s| s.parse::<u8>().unwrap())
                .collect();
            (name, blades)
        })
        .collect();

    for (name, blades) in &structs {
        writeln!(out, "#[derive(Clone, Copy, Default, Debug)]")?;
        writeln!(out, "pub struct {name}<T: Component + Mul<Length>>")?;
        writeln!(out, "where\n    E0<T>: Copy + Default + Debug,\n{{")?;
        for &blade in blades {
            let bname = blade_name(blade);
            let btype = if blade & 1 == 1 { "E0<T>" } else { "T" };
            writeln!(out, "    pub {bname}: {btype},")?;
        }
        writeln!(out, "}}")?;

        generate_arith_impls(out, name, blades)?;
        generate_reverse_impl(out, name, blades)?;
        generate_norm_impl(out, name, blades)?;
        generate_into_values_impl(out, name, blades)?;
    }

    for (&name_in, blades_in) in &structs {
        for (&name_out, blades_out) in &structs {
            generate_conversion_impl(out, (name_in, blades_in), (name_out, blades_out))?;
        }
    }

    structs.insert("Scalar", vec![0]);
    Ok(structs)
}

fn generate_arith_impls(out: &mut File, name: &str, blades: &[u8]) -> Result<()> {
    for (trait_name, fn_name, op) in [("Add", "add", '+'), ("Sub", "sub", '-')] {
        writeln!(
            out,
            "impl<T: Component + Mul<Length>> ::std::ops::{trait_name} for {name}<T>"
        )?;
        writeln!(out, "where\n    E0<T>: Component,\n{{")?;
        writeln!(out, "    type Output = Self;")?;
        writeln!(out, "    #[inline]")?;
        writeln!(out, "    fn {fn_name}(self, rhs: Self) -> Self {{")?;
        writeln!(out, "        Self {{")?;
        for &blade in blades {
            let bname = blade_name(blade);
            writeln!(out, "            {bname}: self.{bname} {op} rhs.{bname},")?;
        }
        writeln!(out, "        }}\n    }}\n}}")?;
    }

    writeln!(
        out,
        "impl<T: Component + Mul<Length>> ::std::ops::Neg for {name}<T>"
    )?;
    writeln!(out, "where\n    E0<T>: Component,\n{{")?;
    writeln!(out, "    type Output = Self;")?;
    writeln!(out, "    #[inline]")?;
    writeln!(out, "    fn neg(self) -> Self {{")?;
    writeln!(out, "        Self {{")?;
    for &blade in blades {
        let bname = blade_name(blade);
        writeln!(out, "            {bname}: -self.{bname},")?;
    }
    writeln!(out, "        }}\n    }}\n}}")?;

    writeln!(
        out,
        "\
impl<T, U> std::ops::Div<Scalar<U>> for {name}<T>
where
    T: Component + Mul<Length> + ::std::ops::Div<U>,
    U: Component + Mul<Length>,
    E0<T>: Component + ::std::ops::Div<U, Output = E0<<T as ::std::ops::Div<U>>::Output>>,
    <T as ::std::ops::Div<U>>::Output: Component + Mul<Length>,
    E0<<T as ::std::ops::Div<U>>::Output>: Component,
{{
    type Output = {name}<<T as ::std::ops::Div<U>>::Output>;
    #[inline]
    fn div(self, rhs: Scalar<U>) -> Self::Output {{
        Self::Output {{
"
    )?;
    for &blade in blades {
        let bname = blade_name(blade);
        writeln!(out, "            {bname}: self.{bname} / rhs,")?;
    }
    writeln!(out, "        }}\n    }}\n}}")?;

    Ok(())
}

fn generate_reverse_impl(out: &mut File, name: &str, blades: &[u8]) -> Result<()> {
    writeln!(
        out,
        "impl<T: Component + Mul<Length>> Reverse for {name}<T>"
    )?;
    writeln!(out, "where\n    E0<T>: Component,\n{{")?;
    writeln!(out, "    #[inline]")?;
    writeln!(out, "    fn reverse(self) -> Self {{")?;
    writeln!(out, "        Self {{")?;
    for &blade in blades {
        let bname = blade_name(blade);
        // Negate if this blade reverses to an odd permutation
        let sign = if blade.count_ones() % 4 >= 2 { "-" } else { "" };
        writeln!(out, "            {bname}: {sign}self.{bname},")?;
    }
    writeln!(out, "        }}\n    }}\n}}")?;

    writeln!(
        out,
        "\
impl<T: Component + Mul<Length>> ::std::ops::Not for {name}<T>
where
    E0<T>: Component,
{{
    type Output = Self;
    #[inline]
    fn not(self) -> Self::Output {{
        Reverse::reverse(self)
    }}
}}"
    )?;

    Ok(())
}

fn generate_norm_impl(out: &mut File, name: &str, blades: &[u8]) -> Result<()> {
    writeln!(
        out,
        "\
impl<D, D2> Norm for {name}<Quantity<D>>
where
    D: si::Dimension + ?Sized,
    D2: si::Dimension + ?Sized,
    Quantity<D>: Component + Mul<Length> + Mul<Quantity<D>, Output = Quantity<D2>>,
    E0<Quantity<D>>: Component,
    Quantity<D2>: Add<Output = Quantity<D2>>,
{{
    type Output = Quantity<D>;
    #[inline]
    fn norm(self) -> Quantity<D> {{
        Quantity::<D> {{
            dimension: ::std::marker::PhantomData,
            units: ::std::marker::PhantomData,
            value: self.normsq().value.sqrt(),
        }}
    }}
    #[inline]
    fn normsq(self) -> Quantity<D2> {{"
    )?;

    let expr = blades
        .iter()
        .copied()
        .filter(|&blade| (blade & 1) == 0)
        .map(|blade| {
            let bname = blade_name(blade);
            format!("self.{bname} * self.{bname}")
        })
        .reduce(|a, b| format!("{a}\n        + {b}"))
        .unwrap_or_else(|| "Default::default()".into());
    writeln!(out, "        {expr}")?;

    writeln!(out, "    }}\n}}")?;

    Ok(())
}

fn generate_conversion_impl(
    out: &mut File,
    (name_in, blades_in): (&str, &[u8]),
    (name_out, blades_out): (&str, &[u8]),
) -> Result<()> {
    if name_in == name_out || blades_in.iter().filter(|&x| blades_out.contains(x)).count() == 0 {
        return Ok(());
    }
    writeln!(
        out,
        "impl<T: Component + Mul<Length>> ::std::convert::From<{name_in}<T>> for {name_out}<T>"
    )?;
    writeln!(out, "where\n    E0<T>: Component,\n{{")?;
    writeln!(out, "    #[inline]")?;
    writeln!(out, "    fn from(value: {name_in}<T>) -> Self {{")?;
    writeln!(out, "        Self {{")?;
    for &blade_out in blades_out {
        let bname = blade_name(blade_out);
        let rhs = if blades_in.contains(&blade_out) {
            format!("value.{bname}")
        } else {
            "Default::default()".into()
        };
        writeln!(out, "            {bname}: {rhs},")?;
    }
    writeln!(out, "        }}\n    }}\n}}")?;

    Ok(())
}

fn generate_into_values_impl(out: &mut File, name: &str, blades: &[u8]) -> Result<()> {
    writeln!(
        out,
        "\
impl<T> ConvertValues<T> for {name}<T>
where
    T: Component + ::std::ops::Mul<Length> + ::std::ops::Div<Output = Ratio> + ::std::ops::Mul<f64, Output = T>,
    E0<T>: Component + ::std::ops::Div<Output = Ratio> + ::std::ops::Mul<f64, Output = E0<T>>,
{{
    type Values = [f64; {}];
    #[inline]
    fn into_values(self, _unit: T, _ideal_unit: E0<T>) -> Self::Values {{
        [",
        blades.len()
    )?;

    for &blade in blades {
        let bname = blade_name(blade);
        let ideal = blade & 1 == 1;
        let unit = if ideal { "_ideal_unit" } else { "_unit" };
        writeln!(
            out,
            "            (self.{bname} / {unit}).get::<::uom::si::ratio::ratio>(),"
        )?;
    }
    writeln!(out, "        ]\n    }}")?;

    writeln!(out, "    #[inline]")?;
    writeln!(
        out,
        "    fn from_values(values: Self::Values, _unit: T, _ideal_unit: E0<T>) -> Self {{"
    )?;
    writeln!(out, "        Self {{")?;
    for (i, &blade) in blades.iter().enumerate() {
        let bname = blade_name(blade);
        let ideal = blade & 1 == 1;
        let unit = if ideal { "_ideal_unit" } else { "_unit" };
        writeln!(out, "            {bname}: {unit} * values[{i}],")?;
    }
    writeln!(out, "        }}\n    }}\n}}")?;

    Ok(())
}

fn generate_ga_products<'a>(
    out: &mut File,
    structs: &GaStructs<'a>,
    products_text: &'a str,
) -> Result<Vec<GaProduct<'a>>> {
    products_text
        .split("\n\n")
        .map(|chunk| generate_ga_product(out, structs, chunk))
        .collect()
}

fn generate_ga_product<'a>(
    out: &mut File,
    structs: &GaStructs<'a>,
    product_text: &'a str,
) -> Result<GaProduct<'a>> {
    let mut lines = product_text.lines();
    let header = lines.next().unwrap();
    let (trait_name, rest) = header.split_once(": ").unwrap();
    let (func_name, rest) = rest.split_once(", ").unwrap();
    let (out_type, overload) = rest.split_once(" - ").unwrap_or((rest, ""));
    let overload = Some(overload).filter(|_| !overload.is_empty());

    writeln!(out, "pub trait {trait_name}<Rhs> {{")?;
    writeln!(out, "    type Output;")?;
    writeln!(out, "    #[must_use]")?;
    writeln!(out, "    fn {func_name}(self, rhs: Rhs) -> Self::Output;")?;
    writeln!(out, "}}")?;

    let table: Vec<Vec<u8>> = load_cayley_table(func_name)?;
    let mut terms = <[Vec<(bool, u8, u8)>; 16]>::default();
    for (i, row) in (0u8..).zip(table) {
        for (j, signed_blade) in (0u8..).zip(row) {
            if signed_blade == 32 {
                continue;
            }
            let sign = (signed_blade >> 4) & 1 == 1;
            let blade = signed_blade & 15;
            terms[blade as usize].push((sign, i, j));
        }
    }

    let product = GaProduct {
        name: trait_name,
        func_name,
        out_type,
        overload,
        terms,
    };

    let mut impls: Vec<[&str; 3]> = lines
        .map(|ln| {
            let (lhs, rest) = ln.trim().split_once(' ').unwrap();
            let (rhs, out) = rest.split_once(" -> ").unwrap();
            [lhs, rhs, out]
        })
        .collect();
    for i in 0..impls.len() {
        let [lhs, rhs, out] = impls[i];
        if lhs != rhs {
            impls.push([rhs, lhs, out]);
        }
    }

    for &names in &impls {
        generate_ga_product_impl(out, structs, &product, names)?;
        if let Some(overload_trait) = product.overload
            && names[0] != "Scalar"
        {
            let mut overload_product = product.clone();
            let qualified_name = format!("::std::ops::{overload_trait}");
            overload_product.name = &qualified_name;
            let lower = overload_trait.to_lowercase();
            overload_product.func_name = &lower;
            generate_ga_product_impl(out, structs, &overload_product, names)?;
        }
    }

    Ok(product)
}

fn generate_ga_product_impl<'a>(
    out: &mut File,
    structs: &GaStructs<'a>,
    product: &GaProduct<'a>,
    [lhs_name, rhs_name, out_name]: [&'a str; 3],
) -> Result<()> {
    writeln!(
        out,
        "impl<T, U> {}<{rhs_name}<U>> for {lhs_name}<T>",
        product.name
    )?;
    writeln!(
        out,
        "\
where
    T: Component + Mul<U> + Mul<Length> + Mul<E0<U>, Output = E0<Prod<T, U>>>,
    U: Component + Mul<Length>,
    E0<T>: Component + Mul<U, Output = E0<Prod<T, U>>> + Mul<E0<U>, Output = E0<E0<Prod<T, U>>>>,
    E0<U>: Component,
    Prod<T, U>: Component + Mul<Length>,
    E0<Prod<T, U>>: Component + Mul<Length>,
    E0<E0<Prod<T, U>>>: Component,
{{",
    )?;
    writeln!(out, "    type Output = {out_name}<{}>;", product.out_type)?;
    writeln!(out, "    #[inline]")?;
    writeln!(
        out,
        "    fn {}(self, rhs: {rhs_name}<U>) -> Self::Output {{",
        product.func_name
    )?;

    if out_name != "Scalar" {
        writeln!(out, "        {out_name} {{")?;
    }

    let lhs_blades = structs.get(&lhs_name).unwrap();
    let rhs_blades = structs.get(&rhs_name).unwrap();
    let out_blades = structs.get(&out_name).unwrap();
    for &out_blade in out_blades {
        let name = blade_name(out_blade);
        if out_name == "Scalar" {
            write!(out, "        ")?;
        } else {
            write!(out, "            {name}: ")?;
        }

        let terms = &product.terms[out_blade as usize];

        let expr = terms
            .iter()
            .filter(|(_, lhs, rhs)| lhs_blades.contains(lhs) && rhs_blades.contains(rhs))
            .map(|&(sign, lhs, rhs)| {
                format!(
                    "({}self{} * rhs{})",
                    if sign { "-" } else { "" },
                    if lhs_name == "Scalar" {
                        String::new()
                    } else {
                        format!(".{}", blade_name(lhs))
                    },
                    if rhs_name == "Scalar" {
                        String::new()
                    } else {
                        format!(".{}", blade_name(rhs))
                    },
                )
            })
            .reduce(|a, b| format!("{a} + {b}"))
            .unwrap_or_else(|| "Default::default()".into());
        write!(out, "{expr}")?;
        if out_name != "Scalar" {
            write!(out, ",")?;
        }
        writeln!(out)?;
    }

    if out_name != "Scalar" {
        writeln!(out, "        }}")?;
    }

    writeln!(out, "    }}\n}}")?;

    Ok(())
}

fn load_cayley_table(name: &str) -> Result<Vec<Vec<u8>>> {
    let table_text = get_input(&format!("{name}.csv"))?;

    let table: Vec<Vec<u8>> = table_text
        .lines()
        .map(|ln| ln.split(',').map(|s| s.trim().parse().unwrap()).collect())
        .collect();

    Ok(table)
}

fn generate_blade_constants(out: &mut File, structs: &GaStructs) -> Result<()> {
    writeln!(out, "/// Dimensionless values for all basis blades")?;
    writeln!(out, "pub mod blades {{")?;
    writeln!(out, "    #![allow(non_upper_case_globals)]")?;
    writeln!(out, "    #[allow(clippy::wildcard_imports)]")?;
    writeln!(out, "    use super::*;")?;
    for blade in 0..32 {
        let bname = blade_name(blade);
        let ideal = blade & 1 == 1;

        let Some((type_name, blades)) = structs.iter().find(|(name, blades)| {
            name.to_lowercase().ends_with("vector") && blades.contains(&blade)
        }) else {
            continue;
        };

        let quantity_name = if ideal {
            "LinearNumberDensity"
        } else {
            "Ratio"
        };

        writeln!(
            out,
            "    pub const {bname}: {type_name}<{quantity_name}> = {{"
        )?;

        let (zero_type, ideal_zero_type) = if ideal {
            ("LinearNumberDensity", "Ratio")
        } else {
            ("Ratio", "Length")
        };

        writeln!(
            out,
            "        \
        let _zero = {zero_type} {{
            dimension: std::marker::PhantomData,
            units: std::marker::PhantomData,
            value: 0.0,
        }};
        let _ideal_zero = {ideal_zero_type} {{
            dimension: std::marker::PhantomData,
            units: std::marker::PhantomData,
            value: 0.0,
        }};
        let one = Ratio {{
            dimension: std::marker::PhantomData,
            units: std::marker::PhantomData,
            value: 1.0,
        }};
        {type_name} {{"
        )?;

        for &struct_blade in blades {
            let expr = if struct_blade == blade {
                "one"
            } else if struct_blade & 1 == 1 {
                "_ideal_zero"
            } else {
                "_zero"
            };
            writeln!(out, "            {}: {expr},", blade_name(struct_blade))?;
        }

        writeln!(out, "        }}\n    }};")?;
    }
    writeln!(out, "}}")?;
    Ok(())
}
