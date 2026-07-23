// Generated from nolanderc/wgsl-spec 0.2.0 (MIT), commit 722a608ca9119a9e83558c5b63eca61542717f4e.
// Regenerate with `cargo run -p xtask -- generate-builtins <functions.json>`.

#[derive(Clone, Copy, Debug)]
pub struct BuiltinOverload {
    pub signature: &'static str,
    pub doc: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct BuiltinFn {
    pub name: &'static str,
    pub overloads: &'static [BuiltinOverload],
}

pub static BUILTIN_FUNCTIONS: &[BuiltinFn] = &[
    BuiltinFn {
        name: r#"abs"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn abs ( e: T ) -> T"#,
            doc: r#"The absolute value of e. Component-wise when T is a vector. If e is a floating-point type, then the result is e with a positive sign bit. If e is an unsigned integer scalar type, then the result is e. If e is a signed integer scalar type and evaluates to the largest negative value, then the result is e."#,
        }],
    },
    BuiltinFn {
        name: r#"acos"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn acos ( e: T ) -> T"#,
            doc: r#"Returns the principal value, in radians, of the inverse cosine (cos -1 ) of e. That is, approximates x with 0 ≤ x ≤ π, such that cos ( x ) = e. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"acosh"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn acosh ( x: T ) -> T"#,
            doc: r#"Returns the inverse hyperbolic cosine (cosh -1 ) of x, as a hyperbolic angle. That is, approximates a with 0 ≤ a ≤ ∞, such that cosh ( a ) = x. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"all"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn all ( e: vecN<bool> ) -> bool"#,
                doc: r#"Returns true if each component of e is true."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn all ( e: bool ) -> bool"#,
                doc: r#"Returns e."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"any"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn any ( e: vecN<bool> ) -> bool"#,
                doc: r#"Returns true if any component of e is true."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn any ( e: bool ) -> bool"#,
                doc: r#"Returns e."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"array"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn array<T, N> ( e1: T, ..., eN: T ) -> array<T, N>"#,
                doc: r#"Construction of an array from elements. Note: array< T, N > is constructible because its element count is equal to the number of arguments to the constructor, and hence fully determined at shader-creation time."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn array ( e1: T, ..., eN: T ) -> array<T, N>"#,
                doc: r#"Construction of an array from elements. The component type is inferred from the elements' type. The size of the array is determined by the number of elements."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"arrayLength"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn arrayLength ( p: ptr<storage, array<E>, AM> ) -> u32"#,
            doc: r#"Returns NRuntime, the number of elements in the runtime-sized array. See § 12.3.4 Buffer Binding Determines Runtime-Sized Array Element Count"#,
        }],
    },
    BuiltinFn {
        name: r#"asin"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn asin ( e: T ) -> T"#,
            doc: r#"Returns the principal value, in radians, of the inverse sine (sin -1 ) of e. That is, approximates x with -π/2 ≤ x ≤ π/2, such that sin ( x ) = e. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"asinh"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn asinh ( y: T ) -> T"#,
            doc: r#"Returns the inverse hyperbolic sine (sinh -1 ) of y, as a hyperbolic angle. That is, approximates a such that sinh ( y ) = a. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"atan"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn atan ( e: T ) -> T"#,
            doc: r#"Returns the principal value, in radians, of the inverse tangent (tan -1 ) of e. That is, approximates x with − π/2 ≤ x ≤ π/2, such that tan ( x ) = e. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"atan2"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn atan2 ( y: T, x: T ) -> T"#,
            doc: r#"Returns an angle, in radians, in the interval [-π, π] whose tangent is y ÷ x. The quadrant selected by the result depends on the signs of y and x. For example, the function may be implemented as: atan(y/x) when x > 0 atan(y/x) + π when ( x < 0) and ( y > 0) atan(y/x) - π when ( x < 0) and ( y < 0) Note: The error in the result is unbounded: When abs(x) is very small, e.g. subnormal for its type, At the origin ( x, y ) = (0,0), or When y is subnormal or infinite. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"atanh"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn atanh ( t: T ) -> T"#,
            doc: r#"Returns the inverse hyperbolic tangent (tanh -1 ) of t, as a hyperbolic angle. That is, approximates a such that tanh ( a ) = t. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"atomicAdd"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn atomicAdd ( atomic_ptr: ptr<AS, atomic<T>, read_write>, v: T ) -> T"#,
            doc: r#"Each function performs the following steps atomically: Load the original value pointed to by atomic_ptr. Obtains a new value by performing the operation (e.g. max) from the function name with the value v. Store the new value using atomic_ptr. Each function returns the original value stored in the atomic object."#,
        }],
    },
    BuiltinFn {
        name: r#"atomicAnd"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn atomicAnd ( atomic_ptr: ptr<AS, atomic<T>, read_write>, v: T ) -> T"#,
            doc: r#"Each function performs the following steps atomically: Load the original value pointed to by atomic_ptr. Obtains a new value by performing the operation (e.g. max) from the function name with the value v. Store the new value using atomic_ptr. Each function returns the original value stored in the atomic object."#,
        }],
    },
    BuiltinFn {
        name: r#"atomicCompareExchangeWeak"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn atomicCompareExchangeWeak ( atomic_ptr: ptr<AS, atomic<T>, read_write>, cmp: T, v: T ) -> __atomic_compare_exchange_result<T>"#,
            doc: r#"Note: A value cannot be explicitly declared with the type __atomic_compare_exchange_result, but a value may infer the type. Performs the following steps atomically: Load the original value pointed to by atomic_ptr. Compare the original value to the value cmp using an equality operation. Store the value v only if the result of the equality comparison was true. Returns a two member structure, where the first member, old_value, is the original value of the atomic object and the second member, exchanged, is whether or not the comparison succeeded. Note: The equality comparison may spuriously fail on some implementations. That is, the second component of the result vector may be false even if the first component of the result vector equals cmp."#,
        }],
    },
    BuiltinFn {
        name: r#"atomicExchange"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn atomicExchange ( atomic_ptr: ptr<AS, atomic<T>, read_write>, v: T ) -> T"#,
            doc: r#"Atomically stores the value v in the atomic object pointed to atomic_ptr and returns the original value stored in the atomic object."#,
        }],
    },
    BuiltinFn {
        name: r#"atomicLoad"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn atomicLoad ( atomic_ptr: ptr<AS, atomic<T>, read_write> ) -> T"#,
            doc: r#"Returns the atomically loaded the value pointed to by atomic_ptr. It does not modify the object."#,
        }],
    },
    BuiltinFn {
        name: r#"atomicMax"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn atomicMax ( atomic_ptr: ptr<AS, atomic<T>, read_write>, v: T ) -> T"#,
            doc: r#"Each function performs the following steps atomically: Load the original value pointed to by atomic_ptr. Obtains a new value by performing the operation (e.g. max) from the function name with the value v. Store the new value using atomic_ptr. Each function returns the original value stored in the atomic object."#,
        }],
    },
    BuiltinFn {
        name: r#"atomicMin"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn atomicMin ( atomic_ptr: ptr<AS, atomic<T>, read_write>, v: T ) -> T"#,
            doc: r#"Each function performs the following steps atomically: Load the original value pointed to by atomic_ptr. Obtains a new value by performing the operation (e.g. max) from the function name with the value v. Store the new value using atomic_ptr. Each function returns the original value stored in the atomic object."#,
        }],
    },
    BuiltinFn {
        name: r#"atomicOr"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn atomicOr ( atomic_ptr: ptr<AS, atomic<T>, read_write>, v: T ) -> T"#,
            doc: r#"Each function performs the following steps atomically: Load the original value pointed to by atomic_ptr. Obtains a new value by performing the operation (e.g. max) from the function name with the value v. Store the new value using atomic_ptr. Each function returns the original value stored in the atomic object."#,
        }],
    },
    BuiltinFn {
        name: r#"atomicStore"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn atomicStore ( atomic_ptr: ptr<AS, atomic<T>, read_write>, v: T )"#,
            doc: r#"Atomically stores the value v in the atomic object pointed to by atomic_ptr."#,
        }],
    },
    BuiltinFn {
        name: r#"atomicSub"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn atomicSub ( atomic_ptr: ptr<AS, atomic<T>, read_write>, v: T ) -> T"#,
            doc: r#"Each function performs the following steps atomically: Load the original value pointed to by atomic_ptr. Obtains a new value by performing the operation (e.g. max) from the function name with the value v. Store the new value using atomic_ptr. Each function returns the original value stored in the atomic object."#,
        }],
    },
    BuiltinFn {
        name: r#"atomicXor"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn atomicXor ( atomic_ptr: ptr<AS, atomic<T>, read_write>, v: T ) -> T"#,
            doc: r#"Each function performs the following steps atomically: Load the original value pointed to by atomic_ptr. Obtains a new value by performing the operation (e.g. max) from the function name with the value v. Store the new value using atomic_ptr. Each function returns the original value stored in the atomic object."#,
        }],
    },
    BuiltinFn {
        name: r#"bitcast"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn bitcast<T> ( e: T ) -> T"#,
                doc: r#"Identity transform. Component-wise when T is a vector. The result is e."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn bitcast<T> ( e: S ) -> T"#,
                doc: r#"Reinterpretation of bits as T. The result is the reintepretation of bits in e as a T value."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn bitcast<vecN<T>> ( e: vecN<S> ) -> vecN<T>"#,
                doc: r#"Component-wise reinterpretation of bits as T. The result is the reintepretation of bits in e as a vecN<T> value."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn bitcast<u32> ( e: AbstractInt ) -> u32 @const @must_use fn bitcast<vecN<u32>> ( e: vecN<AbstractInt> ) -> vecN<u32>"#,
                doc: r#"The identity operation if e can be represented as u32, otherwise it produces a shader-creation error. That is, produces the same result as u32(e). Component-wise when e is a vector."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn bitcast<T> ( e: vec2<f16> ) -> T"#,
                doc: r#"Component-wise reinterpretation of bits as T. The result is the reintepretation of the 32 bits in e as a T value, following the internal layout rules."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn bitcast<vec2<T>> ( e: vec4<f16> ) -> vec2<T>"#,
                doc: r#"Component-wise reinterpretation of bits as T. The result is the reintepretation of the 64 bits in e as a T value, following the internal layout rules."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn bitcast<vec2<f16>> ( e: T ) -> vec2<f16>"#,
                doc: r#"Component-wise reinterpretation of bits as f16. The result is the reintepretation of the 32 bits in e as an f16 value, following the internal layout rules."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn bitcast<vec4<f16>> ( e: vec2<T> ) -> vec4<f16>"#,
                doc: r#"Component-wise reinterpretation of bits as vec2<f16>. The result is the reintepretation of the 64 bits in e as an f16 value, following the internal layout rules."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"bool"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn bool ( e: T ) -> bool"#,
            doc: r#"Construct a bool value. If T is bool, this is an identity operation. Otherwise this is a boolean coercion. The result is false if e is a zero value (or -0.0 for floating point types) and true otherwise."#,
        }],
    },
    BuiltinFn {
        name: r#"ceil"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn ceil ( e: T ) -> T"#,
            doc: r#"Returns the ceiling of e. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"clamp"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn clamp ( e: T, low: T, high: T ) -> T"#,
            doc: r#"Restricts the value of e within a range. If T is an integer type, then the result is min(max(e, low), high). If T is a floating-point type, then the result is either min(max(e, low), high), or the median of the three values e, low, high. Component-wise when T is a vector. If low is greater than high, then: It is a shader-creation error if low and high are const-expressions. It is a pipeline-creation error if low and high are override-expressions."#,
        }],
    },
    BuiltinFn {
        name: r#"cos"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn cos ( e: T ) -> T"#,
            doc: r#"Returns the cosine of e, where e is in radians. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"cosh"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn cosh ( a: T ) -> T"#,
            doc: r#"Returns the hyperbolic cosine of a, where a is a hyperbolic angle. Approximates the pure mathematical function ( e a + e −a )÷2, but not necessarily computed that way. Component-wise when T is a vector"#,
        }],
    },
    BuiltinFn {
        name: r#"countLeadingZeros"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn countLeadingZeros ( e: T ) -> T"#,
            doc: r#"The number of consecutive 0 bits starting from the most significant bit of e, when T is a scalar type. Component-wise when T is a vector. Also known as "clz" in some languages."#,
        }],
    },
    BuiltinFn {
        name: r#"countOneBits"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn countOneBits ( e: T ) -> T"#,
            doc: r#"The number of 1 bits in the representation of e. Also known as "population count". Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"countTrailingZeros"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn countTrailingZeros ( e: T ) -> T"#,
            doc: r#"The number of consecutive 0 bits starting from the least significant bit of e, when T is a scalar type. Component-wise when T is a vector. Also known as "ctz" in some languages."#,
        }],
    },
    BuiltinFn {
        name: r#"cross"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn cross ( e1: vec3<T>, e2: vec3<T> ) -> vec3<T>"#,
            doc: r#"Returns the cross product of e1 and e2."#,
        }],
    },
    BuiltinFn {
        name: r#"degrees"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn degrees ( e1: T ) -> T"#,
            doc: r#"Converts radians to degrees, approximating e1 × 180 ÷ π. Component-wise when T is a vector"#,
        }],
    },
    BuiltinFn {
        name: r#"determinant"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn determinant ( e: matCxC<T> ) -> T"#,
            doc: r#"Returns the determinant of e."#,
        }],
    },
    BuiltinFn {
        name: r#"distance"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn distance ( e1: T, e2: T ) -> S"#,
            doc: r#"Returns the distance between e1 and e2 (e.g. length(e1 - e2) )."#,
        }],
    },
    BuiltinFn {
        name: r#"dot"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn dot ( e1: vecN<T>, e2: vecN<T> ) -> T"#,
            doc: r#"Returns the dot product of e1 and e2."#,
        }],
    },
    BuiltinFn {
        name: r#"dot4I8Packed"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn dot4I8Packed ( e1: u32, e2: u32 ) -> i32"#,
            doc: r#"e1 and e2 are interpreted as vectors with four 8-bit signed integer components. Return the signed integer dot product of these two vectors. Each component is sign-extended to i32 before performing the multiply, and then the add operations are done in WGSL i32 with wrapping behaviour."#,
        }],
    },
    BuiltinFn {
        name: r#"dot4U8Packed"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn dot4U8Packed ( e1: u32, e2: u32 ) -> u32"#,
            doc: r#"e1 and e2 are interpreted as vectors with four 8-bit unsigned integer components. Return the unsigned integer dot product of these two vectors."#,
        }],
    },
    BuiltinFn {
        name: r#"dpdx"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn dpdx ( e: T ) -> T"#,
            doc: r#"Partial derivative of e with respect to window x coordinates. The result is the same as either dpdxFine(e) or dpdxCoarse(e). Returns an indeterminate value if called in non-uniform control flow."#,
        }],
    },
    BuiltinFn {
        name: r#"dpdxCoarse"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn dpdxCoarse ( e: T ) -> T"#,
            doc: r#"Returns the partial derivative of e with respect to window x coordinates using local differences. This may result in fewer unique positions than dpdxFine(e). Returns an indeterminate value if called in non-uniform control flow."#,
        }],
    },
    BuiltinFn {
        name: r#"dpdxFine"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn dpdxFine ( e: T ) -> T"#,
            doc: r#"Returns the partial derivative of e with respect to window x coordinates. Returns an indeterminate value if called in non-uniform control flow."#,
        }],
    },
    BuiltinFn {
        name: r#"dpdy"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn dpdy ( e: T ) -> T"#,
            doc: r#"Partial derivative of e with respect to window y coordinates. The result is the same as either dpdyFine(e) or dpdyCoarse(e). Returns an indeterminate value if called in non-uniform control flow."#,
        }],
    },
    BuiltinFn {
        name: r#"dpdyCoarse"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn dpdyCoarse ( e: T ) -> T"#,
            doc: r#"Returns the partial derivative of e with respect to window y coordinates using local differences. This may result in fewer unique positions than dpdyFine(e). Returns an indeterminate value if called in non-uniform control flow."#,
        }],
    },
    BuiltinFn {
        name: r#"dpdyFine"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn dpdyFine ( e: T ) -> T"#,
            doc: r#"Returns the partial derivative of e with respect to window y coordinates. Returns an indeterminate value if called in non-uniform control flow."#,
        }],
    },
    BuiltinFn {
        name: r#"exp"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn exp ( e1: T ) -> T"#,
            doc: r#"Returns the natural exponentiation of e1 (e.g. e e1 ). Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"exp2"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn exp2 ( e: T ) -> T"#,
            doc: r#"Returns 2 raised to the power e (e.g. 2 e ). Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"extractBits"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn extractBits ( e: T, offset: u32, count: u32 ) -> T"#,
                doc: r#"Reads bits from an integer, with sign extension. When T is a scalar type, then: w is the bit width of T o = min(offset, w) c = min(count, w - o) The result is 0 if c is 0. Otherwise, bits 0..c - 1 of the result are copied from bits o..o + c - 1 of e. Other bits of the result are the same as bit c - 1 of the result. Component-wise when T is a vector. If count + offset is greater than w, then: It is a shader-creation error if count and offset are const-expressions. It is a pipeline-creation error if count and offset are override-expressions."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn extractBits ( e: T, offset: u32, count: u32 ) -> T"#,
                doc: r#"Reads bits from an integer, without sign extension. When T is a scalar type, then: w is the bit width of T o = min(offset, w) c = min(count, w - o) The result is 0 if c is 0. Otherwise, bits 0..c - 1 of the result are copied from bits o..o + c - 1 of e. Other bits of the result are 0. Component-wise when T is a vector. If count + offset is greater than w, then: It is a shader-creation error if count and offset are const-expressions. It is a pipeline-creation error if count and offset are override-expressions."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"f16"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn f16 ( e: T ) -> f16"#,
            doc: r#"Construct an f16 value. If T is f16, this is an identity operation. If T is a numeric scalar (other than f16 ), e is converted to f16 (including invalid conversions). If T is bool, the result is 1.0h if e is true and 0.0h otherwise."#,
        }],
    },
    BuiltinFn {
        name: r#"f32"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn f32 ( e: T ) -> f32"#,
            doc: r#"Construct an f32 value. If T is f32, this is an identity operation. If T is a numeric scalar (other than f32 ), e is converted to f32 (including invalid conversions). If T is bool, the result is 1.0f if e is true and 0.0f otherwise."#,
        }],
    },
    BuiltinFn {
        name: r#"faceForward"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn faceForward ( e1: T, e2: T, e3: T ) -> T"#,
            doc: r#"Returns e1 if dot(e2, e3) is negative, and -e1 otherwise."#,
        }],
    },
    BuiltinFn {
        name: r#"firstLeadingBit"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn firstLeadingBit ( e: T ) -> T"#,
                doc: r#"For scalar T, the result is: -1 if e is 0 or -1. Otherwise the position of the most significant bit in e that is different from e ’s sign bit. Component-wise when T is a vector."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn firstLeadingBit ( e: T ) -> T"#,
                doc: r#"For scalar T, the result is: T(-1) if e is zero. Otherwise the position of the most significant 1 bit in e. Component-wise when T is a vector."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"firstTrailingBit"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn firstTrailingBit ( e: T ) -> T"#,
            doc: r#"For scalar T, the result is: T(-1) if e is zero. Otherwise the position of the least significant 1 bit in e. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"floor"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn floor ( e: T ) -> T"#,
            doc: r#"Returns the floor of e. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"fma"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn fma ( e1: T, e2: T, e3: T ) -> T"#,
            doc: r#"Returns e1 * e2 + e3. Component-wise when T is a vector. Note: The name fma is short for "fused multiply add". Note: The IEEE-754 fusedMultiplyAdd operation computes the intermediate results as if with unbounded range and precision, and only the final result is rounded to the destination type. However, the § 14.6.2 Floating Point Accuracy rule for fma allows an implementation which performs an ordinary multiply to the target type followed by an ordinary addition. In this case the intermediate values may overflow or lose accuracy, and the overall operation is not "fused" at all."#,
        }],
    },
    BuiltinFn {
        name: r#"fract"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn fract ( e: T ) -> T"#,
            doc: r#"Returns the fractional part of e, computed as e - floor(e). Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"frexp"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn frexp ( e: T ) -> __frexp_result_f32"#,
                doc: r#"Splits e into a fraction and an exponent. When e is zero, the fraction is zero. When e is non-zero and normal, e = fraction * 2 exponent, where the fraction is in the range [0.5, 1.0) or (-1.0, -0.5]. Otherwise, e is denormalized, NaN, or infinite. The result fraction and exponent are indeterminate values. Returns the __frexp_result_f32 built-in structure, defined as follows: struct __frexp_result_f32 { fract: f32, // fraction part exp: i32 // exponent part } Note: A mnemonic for the name frexp is " fr action and exp onent"."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn frexp ( e: T ) -> __frexp_result_f16"#,
                doc: r#"Splits e into a fraction and an exponent. When e is zero, the fraction is zero. When e is non-zero and normal, e = fraction * 2 exponent, where the fraction is in the range [0.5, 1.0) or (-1.0, -0.5]. Otherwise, e is denormalized, NaN, or infinite. The result fraction and exponent are indeterminate values. Returns the __frexp_result_f16 built-in structure, defined as if as follows: struct __frexp_result_f16 { fract: f16, // fraction part exp: i32 // exponent part } Note: A mnemonic for the name frexp is " fr action and exp onent"."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn frexp ( e: T ) -> __frexp_result_abstract"#,
                doc: r#"Splits e into a fraction and an exponent. When e is zero, the fraction is zero. When e is non-zero and normal, e = fraction * 2 exponent, where the fraction is in the range [0.5, 1.0) or (-1.0, -0.5]. When e is denormalized, the fraction and exponent are have unbounded error. The fraction may be any AbstractFloat value, and the exponent may be any AbstractInt value. Note: AbstractFloat expressions resulting in infinity or NaN cause a shader-creation error. Returns the __frexp_result_abstract built-in structure, defined as follows: struct __frexp_result_abstract { fract: AbstractFloat, // fraction part exp: AbstractInt // exponent part } Note: A mnemonic for the name frexp is " fr action and exp onent"."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn frexp ( e: T ) -> __frexp_result_vecN_f32"#,
                doc: r#"Splits components ei of e into a fraction and an exponent. When ei is zero, the fraction is zero. When ei is non-zero and normal, ei = fraction * 2 exponent, where the fraction is in the range [0.5, 1.0) or (-1.0, -0.5]. Otherwise, ei is NaN or infinite. The result fraction and exponent are indeterminate values. Returns the __frexp_result_vecN_f32 built-in structure, defined as follows: struct __frexp_result_vecN_f32 { fract: vecN < f32 >, // fraction part exp: vecN < i32 > // exponent part } Note: A mnemonic for the name frexp is " fr action and exp onent"."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn frexp ( e: T ) -> __frexp_result_vecN_f16"#,
                doc: r#"Splits components ei of e into a fraction and an exponent. When ei is zero, the fraction is zero. When ei is non-zero and normal, ei = fraction * 2 exponent, where the fraction is in the range [0.5, 1.0) or (-1.0, -0.5]. Otherwise, ei is NaN or infinite. The result fraction and exponent are indeterminate values. Returns the __frexp_result_vecN_f16 built-in structure, defined as if as follows: struct __frexp_result_vecN_f16 { fract: vecN < f16 >, // fraction part exp: vecN < i32 > // exponent part } Note: A mnemonic for the name frexp is " fr action and exp onent"."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn frexp ( e: T ) -> __frexp_result_vecN_abstract"#,
                doc: r#"Splits components ei of e into a fraction and an exponent. When ei is zero, the fraction is zero. When ei is non-zero and normal, ei = fraction * 2 exponent, where the fraction is in the range [0.5, 1.0) or (-1.0, -0.5]. When ei is denormalized, the fraction and exponent are have unbounded error. The fraction may be any AbstractFloat value, and the exponent may be any AbstractInt value. Note: AbstractFloat expressions resulting in infinity or NaN cause a shader-creation error. Returns the __frexp_result_vecN_abstract built-in structure, defined as follows: struct __frexp_result_vecN_abstract { fract: vecN < AbstractFloat >, // fraction part exp: vecN < AbstractInt > // exponent part } Note: A mnemonic for the name frexp is " fr action and exp onent"."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"fwidth"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn fwidth ( e: T ) -> T"#,
            doc: r#"Returns abs(dpdx(e)) + abs(dpdy(e)). Returns an indeterminate value if called in non-uniform control flow."#,
        }],
    },
    BuiltinFn {
        name: r#"fwidthCoarse"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn fwidthCoarse ( e: T ) -> T"#,
            doc: r#"Returns abs(dpdxCoarse(e)) + abs(dpdyCoarse(e)). Returns an indeterminate value if called in non-uniform control flow."#,
        }],
    },
    BuiltinFn {
        name: r#"fwidthFine"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn fwidthFine ( e: T ) -> T"#,
            doc: r#"Returns abs(dpdxFine(e)) + abs(dpdyFine(e)). Returns an indeterminate value if called in non-uniform control flow."#,
        }],
    },
    BuiltinFn {
        name: r#"i32"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn i32 ( e: T ) -> i32"#,
            doc: r#"Construct an i32 value. If T is i32, this is an identity operation. If T is u32, this is a reinterpretation of bits (i.e. the result is the unique value in i32 that has the same bit pattern as e ). If T is a floating point type, e is converted to i32, rounding towards zero. If T is bool, the result is 1i if e is true and 0i otherwise. If T is an AbstractInt, this is an identity operation if e can be represented in i32, otherwise it produces a shader-creation error."#,
        }],
    },
    BuiltinFn {
        name: r#"insertBits"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn insertBits ( e: T, newbits: T, offset: u32, count: u32 ) -> T"#,
            doc: r#"Sets bits in an integer. When T is a scalar type, then: w is the bit width of T o = min(offset, w) c = min(count, w - o) The result is e if c is 0. Otherwise, bits o..o + c - 1 of the result are copied from bits 0..c - 1 of newbits. Other bits of the result are copied from e. Component-wise when T is a vector. If count + offset is greater than w, then: It is a shader-creation error if count and offset are const-expressions. It is a pipeline-creation error if count and offset are override-expressions."#,
        }],
    },
    BuiltinFn {
        name: r#"inverseSqrt"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn inverseSqrt ( e: T ) -> T"#,
            doc: r#"Returns the reciprocal of sqrt(e). Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"ldexp"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn ldexp ( e1: T, e2: I ) -> T"#,
            doc: r#"Returns e1 * 2 e2, except: The result may be zero if e2 + bias ≤ 0. If e2 > bias + 1 It is a shader-creation error if e2 is a const-expression. It is a pipeline-creation error if e2 is an override-expression. Otherwise the result is an indeterminate value for T. Here, bias is the exponent bias of the floating point format: 15 for f16 127 for f32 1023 for AbstractFloat, when AbstractFloat is IEEE-754 binary64 If x is zero or a finite normal value for its type, then: x = ldexp(frexp(x).fract, frexp(x).exp) Component-wise when T is a vector. Note: A mnemonic for the name ldexp is "load exponent". The name may have been taken from the corresponding instruction in the floating point unit of the PDP-11."#,
        }],
    },
    BuiltinFn {
        name: r#"length"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn length ( e: T ) -> S"#,
            doc: r#"Returns the length of e. Evaluates to the absolute value of e if T is scalar. Evaluates to sqrt(e[0] 2 + e[1] 2 + ...) if T is a vector type. Note: The scalar case may be evaluated as sqrt(e * e), which may unnecessarily overflow or lose accuracy."#,
        }],
    },
    BuiltinFn {
        name: r#"log"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn log ( e: T ) -> T"#,
            doc: r#"Returns the natural logarithm of e. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"log2"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn log2 ( e: T ) -> T"#,
            doc: r#"Returns the base-2 logarithm of e. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"mat2x2"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn mat2x2<T> ( e: mat2x2<S> ) -> mat2x2<T> @const @must_use fn mat2x2 ( e: mat2x2<S> ) -> mat2x2<S>"#,
                doc: r#"Constructor for a 2x2 column-major matrix. If T does not match S, a conversion occurs."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat2x2<T> ( v1: vec2<T>, v2: vec2<T> ) -> mat2x2<T> @const @must_use fn mat2x2 ( v1: vec2<T>, v2: vec2<T> ) -> mat2x2<T>"#,
                doc: r#"Construct a 2x2 column-major matrix from column vectors."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat2x2<T> ( e1: T, e2: T, e3: T, e4: T ) -> mat2x2<T> @const @must_use fn mat2x2 ( e1: T, e2: T, e3: T, e4: T ) -> mat2x2<T>"#,
                doc: r#"Construct a 2x2 column-major matrix from elements. Same as mat2x2(vec2(e1,e2), vec2(e3,e4))."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"mat2x3"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn mat2x3<T> ( e: mat2x3<S> ) -> mat2x3<T> @const @must_use fn mat2x3 ( e: mat2x3<S> ) -> mat2x3<S>"#,
                doc: r#"Constructor for a 2x3 column-major matrix. If T does not match S, a conversion occurs."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat2x3<T> ( v1: vec3<T>, v2: vec3<T> ) -> mat2x3<T> @const @must_use fn mat2x3 ( v1: vec3<T>, v2: vec3<T> ) -> mat2x3<T>"#,
                doc: r#"Construct a 2x3 column-major matrix from column vectors."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat2x3<T> ( e1: T, ..., e6: T ) -> mat2x3<T> @const @must_use fn mat2x3 ( e1: T, ..., e6: T ) -> mat2x3<T>"#,
                doc: r#"Construct a 2x3 column-major matrix from elements. Same as mat2x3(vec3(e1,e2,e3), vec3(e4,e5,e6))."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"mat2x4"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn mat2x4<T> ( e: mat2x4<S> ) -> mat2x4<T> @const @must_use fn mat2x4 ( e: mat2x4<S> ) -> mat2x4<S>"#,
                doc: r#"Constructor for a 2x4 column-major matrix. If T does not match S, a conversion occurs."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat2x4<T> ( v1: vec4<T>, v2: vec4<T> ) -> mat2x4<T> @const @must_use fn mat2x4 ( v1: vec4<T>, v2: vec4<T> ) -> mat2x4<T>"#,
                doc: r#"Construct a 2x4 column-major matrix from column vectors."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat2x4<T> ( e1: T, ..., e8: T ) -> mat2x4<T> @const @must_use fn mat2x4 ( e1: T, ..., e8: T ) -> mat2x4<T>"#,
                doc: r#"Construct a 2x4 column-major matrix from elements. Same as mat2x4(vec4(e1,e2,e3,e4), vec4(e5,e6,e7,e8))."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"mat3x2"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn mat3x2<T> ( e: mat3x2<S> ) -> mat3x2<T> @const @must_use fn mat3x2 ( e: mat3x2<S> ) -> mat3x2<S>"#,
                doc: r#"Constructor for a 3x2 column-major matrix. If T does not match S, a conversion occurs."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat3x2<T> ( v1: vec2<T>, v2: vec2<T>, v3: vec2<T> ) -> mat3x2<T> @const @must_use fn mat3x2 ( v1: vec2<T>, v2: vec2<T>, v3: vec2<T> ) -> mat3x2<T>"#,
                doc: r#"Construct a 3x2 column-major matrix from column vectors."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat3x2<T> ( e1: T, ..., e6: T ) -> mat3x2<T> @const @must_use fn mat3x2 ( e1: T, ..., e6: T ) -> mat3x2<T>"#,
                doc: r#"Construct a 3x2 column-major matrix from elements. Same as mat3x2(vec2(e1,e2), vec2(e3,e4), vec2(e5,e6))."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"mat3x3"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn mat3x3<T> ( e: mat3x3<S> ) -> mat3x3<T> @const @must_use fn mat3x3 ( e: mat3x3<S> ) -> mat3x3<S>"#,
                doc: r#"Constructor for a 3x3 column-major matrix. If T does not match S, a conversion occurs."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat3x3<T> ( v1: vec3<T>, v2: vec3<T>, v3: vec3<T> ) -> mat3x3<T> @const @must_use fn mat3x3 ( v1: vec3<T>, v2: vec3<T>, v3: vec3<T> ) -> mat3x3<T>"#,
                doc: r#"Construct a 3x3 column-major matrix from column vectors."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat3x3<T> ( e1: T, ..., e9: T ) -> mat3x3<T> @const @must_use fn mat3x3 ( e1: T, ..., e9: T ) -> mat3x3<T>"#,
                doc: r#"Construct a 3x3 column-major matrix from elements. Same as mat3x3(vec3(e1,e2,e3), vec3(e4,e4,e6), vec3(e7,e8,e9))."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"mat3x4"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn mat3x4<T> ( e: mat3x4<S> ) -> mat3x4<T> @const @must_use fn mat3x4 ( e: mat3x4<S> ) -> mat3x4<S>"#,
                doc: r#"Constructor for a 3x4 column-major matrix. If T does not match S, a conversion occurs."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat3x4<T> ( v1: vec4<T>, v2: vec4<T>, v3: vec4<T> ) -> mat3x4<T> @const @must_use fn mat3x4 ( v1: vec4<T>, v2: vec4<T>, v3: vec4<T> ) -> mat3x4<T>"#,
                doc: r#"Construct a 3x4 column-major matrix from column vectors."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat3x4<T> ( e1: T, ..., e12: T ) -> mat3x4<T> @const @must_use fn mat3x4 ( e1: T, ..., e12: T ) -> mat3x4<T>"#,
                doc: r#"Construct a 3x4 column-major matrix from elements. Same as mat3x4(vec4(e1,e2,e3,e4), vec4(e5,e6,e7,e8), vec4(e9,e10,e11,e12))."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"mat4x2"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn mat4x2<T> ( e: mat4x2<S> ) -> mat4x2<T> @const @must_use fn mat4x2 ( e: mat4x2<S> ) -> mat4x2<S>"#,
                doc: r#"Constructor for a 4x2 column-major matrix. If T does not match S, a conversion occurs."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat4x2<T> ( v1: vec2<T>, v2: vec2<T>, v3: vec2<T>, v4: vec2<T> ) -> mat4x2<T> @const @must_use fn mat4x2 ( v1: vec2<T>, v2: vec2<T>, v3: vec2<T>, v4: vec2<T> ) -> mat4x2<T>"#,
                doc: r#"Construct a 4x2 column-major matrix from column vectors."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat4x2<T> ( e1: T, ..., e8: T ) -> mat4x2<T> @const @must_use fn mat4x2 ( e1: T, ..., e8: T ) -> mat4x2<T>"#,
                doc: r#"Construct a 4x2 column-major matrix from elements. Same as mat4x2(vec2(e1,e2), vec2(e3,e4), vec2(e5,e6), vec2(e7,e8))."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"mat4x3"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn mat4x3<T> ( e: mat4x3<S> ) -> mat4x3<T> @const @must_use fn mat4x3 ( e: mat4x3<S> ) -> mat4x3<S>"#,
                doc: r#"Constructor for a 4x3 column-major matrix. If T does not match S, a conversion occurs."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat4x3<T> ( v1: vec3<T>, v2: vec3<T>, v3: vec3<T>, v4: vec3<T> ) -> mat4x3<T> @const @must_use fn mat4x3 ( v1: vec3<T>, v2: vec3<T>, v3: vec3<T>, v4: vec3<T> ) -> mat4x3<T>"#,
                doc: r#"Construct a 4x3 column-major matrix from column vectors."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat4x3<T> ( e1: T, ..., e12: T ) -> mat4x3<T> @const @must_use fn mat4x3 ( e1: T, ..., e12: T ) -> mat4x3<T>"#,
                doc: r#"Construct a 4x3 column-major matrix from elements. Same as mat4x3(vec3(e1,e2,e3), vec3(e4,e5,e6), vec3(e7,e8,e9), vec3(e10,e11,e12))."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"mat4x4"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn mat4x4<T> ( e: mat4x4<S> ) -> mat4x4<T> @const @must_use fn mat4x4 ( e: mat4x4<S> ) -> mat4x4<S>"#,
                doc: r#"Constructor for a 4x4 column-major matrix. If T does not match S, a conversion occurs."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat4x4<T> ( v1: vec4<T>, v2: vec4<T>, v3: vec4<T>, v4: vec4<T> ) -> mat4x4<T> @const @must_use fn mat4x4 ( v1: vec4<T>, v2: vec4<T>, v3: vec4<T>, v4: vec4<T> ) -> mat4x4<T>"#,
                doc: r#"Construct a 4x4 column-major matrix from column vectors."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mat4x4<T> ( e1: T, ..., e16: T ) -> mat4x4<T> @const @must_use fn mat4x4 ( e1: T, ..., e16: T ) -> mat4x4<T>"#,
                doc: r#"Construct a 4x4 column-major matrix from elements. Same as mat4x4(vec4(e1,e2,e3,e4), vec4(e5,e6,e7,e8), vec4(e9,e10,e11,e12), vec4(e13,e14,e15,e16))."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"max"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn max ( e1: T, e2: T ) -> T"#,
            doc: r#"Returns e2 if e1 is less than e2, and e1 otherwise. Component-wise when T is a vector. If e1 and e2 are floating-point values, then: If both e1 and e2 are denormalized, then the result may be either value. If one operand is a NaN, the other is returned. If both operands are NaNs, a NaN is returned."#,
        }],
    },
    BuiltinFn {
        name: r#"min"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn min ( e1: T, e2: T ) -> T"#,
            doc: r#"Returns e2 if e2 is less than e1, and e1 otherwise. Component-wise when T is a vector. If e1 and e2 are floating-point values, then: If both e1 and e2 are denormalized, then the result may be either value. If one operand is a NaN, the other is returned. If both operands are NaNs, a NaN is returned."#,
        }],
    },
    BuiltinFn {
        name: r#"mix"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn mix ( e1: T, e2: T, e3: T ) -> T"#,
                doc: r#"Returns the linear blend of e1 and e2 (e.g. e1 * (1 - e3) + e2 * e3 ). Component-wise when T is a vector."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn mix ( e1: T2, e2: T2, e3: T ) -> T2"#,
                doc: r#"Returns the component-wise linear blend of e1 and e2, using scalar blending factor e3 for each component. Same as mix(e1, e2, T2(e3))."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"modf"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn modf ( e: T ) -> __modf_result_f32"#,
                doc: r#"Splits e into fractional and whole number parts. The whole part is trunc ( e ), and the fractional part is e - trunc ( e ). Returns the __modf_result_f32 built-in structure, defined as follows: struct __modf_result_f32 { fract: f32, // fractional part whole: f32 // whole part }"#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn modf ( e: T ) -> __modf_result_f16"#,
                doc: r#"Splits e into fractional and whole number parts. The whole part is trunc ( e ), and the fractional part is e - trunc ( e ). Returns the __modf_result_f16 built-in structure, defined as if as follows: struct __modf_result_f16 { fract: f16, // fractional part whole: f16 // whole part }"#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn modf ( e: T ) -> __modf_result_abstract"#,
                doc: r#"Splits e into fractional and whole number parts. The whole part is trunc ( e ), and the fractional part is e - trunc ( e ). Returns the __modf_result_abstract built-in structure, defined as follows: struct __modf_result_abstract { fract: AbstractFloat, // fractional part whole: AbstractFloat // whole part }"#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn modf ( e: T ) -> __modf_result_vecN_f32"#,
                doc: r#"Splits the components of e into fractional and whole number parts. The i ’th component of the whole and fractional parts equal the whole and fractional parts of modf(e[i]). Returns the __modf_result_vecN_f32 built-in structure, defined as follows: struct __modf_result_vecN_f32 { fract: vecN < f32 >, // fractional part whole: vecN < f32 > // whole part }"#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn modf ( e: T ) -> __modf_result_vecN_f16"#,
                doc: r#"Splits the components of e into fractional and whole number parts. The i ’th component of the whole and fractional parts equal the whole and fractional parts of modf(e[i]). Returns the __modf_result_vecN_f16 built-in structure, defined as if as follows: struct __modf_result_vecN_f16 { fract: vecN < f16 >, // fractional part whole: vecN < f16 > // whole part }"#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn modf ( e: T ) -> __modf_result_vecN_abstract"#,
                doc: r#"Splits the components of e into fractional and whole number parts. The i ’th component of the whole and fractional parts equal the whole and fractional parts of modf(e[i]). Returns the __modf_result_vecN_abstract built-in structure, defined as follows: struct __modf_result_vecN_abstract { fract: vecN < AbstractFloat >, // fractional part whole: vecN < AbstractFloat > // whole part }"#,
            },
        ],
    },
    BuiltinFn {
        name: r#"normalize"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn normalize ( e: vecN<T> ) -> vecN<T>"#,
            doc: r#"Returns a unit vector in the same direction as e."#,
        }],
    },
    BuiltinFn {
        name: r#"pack2x16float"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn pack2x16float ( e: vec2<f32> ) -> u32"#,
            doc: r#"Converts two floating point values to half-precision floating point numbers, and then combines them into one u32 value. Component e[i] of the input is converted to a IEEE-754 binary16 value, which is then placed in bits 16 × i through 16 × i + 15 of the result. See § 14.6.4 Floating Point Conversion. If either e[0] or e[1] is outside the finite range of binary16 then: It is a shader-creation error if e is a const-expression. It is a pipeline-creation error if e is an override-expression. Otherwise the result is an indeterminate value for u32."#,
        }],
    },
    BuiltinFn {
        name: r#"pack2x16snorm"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn pack2x16snorm ( e: vec2<f32> ) -> u32"#,
            doc: r#"Converts two normalized floating point values to 16-bit signed integers, and then combines them into one u32 value. Component e[i] of the input is converted to a 16-bit twos complement integer value ⌊ 0.5 + 32767 × min(1, max(-1, e[i])) ⌋ which is then placed in bits 16 × i through 16 × i + 15 of the result."#,
        }],
    },
    BuiltinFn {
        name: r#"pack2x16unorm"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn pack2x16unorm ( e: vec2<f32> ) -> u32"#,
            doc: r#"Converts two normalized floating point values to 16-bit unsigned integers, and then combines them into one u32 value. Component e[i] of the input is converted to a 16-bit unsigned integer value ⌊ 0.5 + 65535 × min(1, max(0, e[i])) ⌋ which is then placed in bits 16 × i through 16 × i + 15 of the result."#,
        }],
    },
    BuiltinFn {
        name: r#"pack4x8snorm"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn pack4x8snorm ( e: vec4<f32> ) -> u32"#,
            doc: r#"Converts four normalized floating point values to 8-bit signed integers, and then combines them into one u32 value. Component e[i] of the input is converted to an 8-bit twos complement integer value ⌊ 0.5 + 127 × min(1, max(-1, e[i])) ⌋ which is then placed in bits 8 × i through 8 × i + 7 of the result."#,
        }],
    },
    BuiltinFn {
        name: r#"pack4x8unorm"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn pack4x8unorm ( e: vec4<f32> ) -> u32"#,
            doc: r#"Converts four normalized floating point values to 8-bit unsigned integers, and then combines them into one u32 value. Component e[i] of the input is converted to an 8-bit unsigned integer value ⌊ 0.5 + 255 × min(1, max(0, e[i])) ⌋ which is then placed in bits 8 × i through 8 × i + 7 of the result."#,
        }],
    },
    BuiltinFn {
        name: r#"pack4xI8"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn pack4xI8 ( e: vec4<i32> ) -> u32"#,
            doc: r#"Pack the lower 8 bits of each component of e into a u32 value and drop all the unused bits."#,
        }],
    },
    BuiltinFn {
        name: r#"pack4xI8Clamp"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn pack4xI8Clamp ( e: vec4<i32> ) -> u32"#,
            doc: r#"Clamp each component of e in the range [-128, 127] and then pack the lower 8 bits of each component into a u32 value."#,
        }],
    },
    BuiltinFn {
        name: r#"pack4xU8"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn pack4xU8 ( e: vec4<u32> ) -> u32"#,
            doc: r#"Pack the lower 8 bits of each component of e into a u32 value and drop all the unused bits."#,
        }],
    },
    BuiltinFn {
        name: r#"pack4xU8Clamp"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn pack4xU8Clamp ( e: vec4<u32> ) -> u32"#,
            doc: r#"Clamp each component of e in the range of [0, 255] and then pack the lower 8 bits of each component into a u32 value."#,
        }],
    },
    BuiltinFn {
        name: r#"pow"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn pow ( e1: T, e2: T ) -> T"#,
            doc: r#"Returns e1 raised to the power e2. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"quantizeToF16"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn quantizeToF16 ( e: T ) -> T"#,
            doc: r#"Quantizes a 32-bit floating point value e as if e were converted to a IEEE 754 binary16 value, and then converted back to a IEEE 754 binary32 value. If e is outside the finite range of binary16, then: It is a shader-creation error if e is a const-expression. It is a pipeline-creation error if e is an override-expression. Otherwise the result is an indeterminate value for T. The intermediate binary16 value may be flushed to zero, i.e. the final result may be zero if the intermediate binary16 value is denormalized. See § 14.6.4 Floating Point Conversion. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"radians"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn radians ( e1: T ) -> T"#,
            doc: r#"Converts degrees to radians, approximating e1 × π ÷ 180. Component-wise when T is a vector"#,
        }],
    },
    BuiltinFn {
        name: r#"reflect"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn reflect ( e1: T, e2: T ) -> T"#,
            doc: r#"For the incident vector e1 and surface orientation e2, returns the reflection direction e1 - 2 * dot(e2, e1) * e2."#,
        }],
    },
    BuiltinFn {
        name: r#"refract"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn refract ( e1: T, e2: T, e3: I ) -> T"#,
            doc: r#"For the incident vector e1 and surface normal e2, and the ratio of indices of refraction e3, let k = 1.0 - e3 * e3 * (1.0 - dot(e2, e1) * dot(e2, e1)). If k < 0.0, returns the refraction vector 0.0, otherwise return the refraction vector e3 * e1 - (e3 * dot(e2, e1) + sqrt(k)) * e2."#,
        }],
    },
    BuiltinFn {
        name: r#"reverseBits"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn reverseBits ( e: T ) -> T"#,
            doc: r#"Reverses the bits in e: The bit at position k of the result equals the bit at position 31 -k of e. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"round"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn round ( e: T ) -> T"#,
            doc: r#"Result is the integer k nearest to e, as a floating point value. When e lies halfway between integers k and k + 1, the result is k when k is even, and k + 1 when k is odd. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"saturate"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn saturate ( e: T ) -> T"#,
            doc: r#"Returns clamp(e, 0.0, 1.0). Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"select"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn select ( f: T, t: T, cond: bool ) -> T"#,
                doc: r#"Returns t when cond is true, and f otherwise."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn select ( f: vecN<T>, t: vecN<T>, cond: vecN<bool> ) -> vecN<T>"#,
                doc: r#"Component-wise selection. Result component i is evaluated as select(f[i], t[i], cond[i])."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"sign"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn sign ( e: T ) -> T"#,
            doc: r#"Result is: 1 when e > 0 0 when e = 0 -1 when e < 0 Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"sin"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn sin ( e: T ) -> T"#,
            doc: r#"Returns the sine of e, where e is in radians. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"sinh"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn sinh ( a: T ) -> T"#,
            doc: r#"Returns the hyperbolic sine of a, where a is a hyperbolic angle. Approximates the pure mathematical function ( e a − e −a )÷2, but not necessarily computed that way. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"smoothstep"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn smoothstep ( low: T, high: T, x: T ) -> T"#,
            doc: r#"Returns the smooth Hermite interpolation between 0 and 1. Component-wise when T is a vector. For scalar T, the result is t * t * (3.0 - 2.0 * t), where t = clamp((x - low) / (high - low), 0.0, 1.0). If low >= high: It is a shader-creation error if low and high are const-expressions. It is a pipeline-creation error if low and high are override-expressions."#,
        }],
    },
    BuiltinFn {
        name: r#"sqrt"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn sqrt ( e: T ) -> T"#,
            doc: r#"Returns the square root of e. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"step"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn step ( edge: T, x: T ) -> T"#,
            doc: r#"Returns 1.0 if edge ≤ x, and 0.0 otherwise. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"storageBarrier"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn storageBarrier ()"#,
            doc: r#"Executes a control barrier synchronization function that affects memory and atomic operations in the storage address space."#,
        }],
    },
    BuiltinFn {
        name: r#"tan"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn tan ( e: T ) -> T"#,
            doc: r#"Returns the tangent of e, where e is in radians. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"tanh"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn tanh ( a: T ) -> T"#,
            doc: r#"Returns the hyperbolic tangent of a, where a is a hyperbolic angle. Approximates the pure mathematical function ( e a − e −a ) ÷ ( e a + e −a ) but not necessarily computed that way. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"textureBarrier"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn textureBarrier ()"#,
            doc: r#"Executes a control barrier synchronization function that affects memory operations in the handle address space."#,
        }],
    },
    BuiltinFn {
        name: r#"textureDimensions"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@must_use fn textureDimensions ( t: T ) -> u32"#,
                doc: r#"Returns the dimensions of a texture, or texture’s mip level in texels.

Returns:

The coordinate dimensions of the texture.

That is, the result provides the integer bounds on the coordinates of the logical texel address, excluding the mip level count, array size, and sample count.

For textures based on cubes, the results are the dimensions of each face of the cube. Cube faces are square, so the x and y components of the result are equal.

If level is outside the range [0, textureNumLevels(t)) then an indeterminate value for the return type may be returned."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureDimensions ( t: T, level: L ) -> u32"#,
                doc: r#"Returns the dimensions of a texture, or texture’s mip level in texels.

Returns:

The coordinate dimensions of the texture.

That is, the result provides the integer bounds on the coordinates of the logical texel address, excluding the mip level count, array size, and sample count.

For textures based on cubes, the results are the dimensions of each face of the cube. Cube faces are square, so the x and y components of the result are equal.

If level is outside the range [0, textureNumLevels(t)) then an indeterminate value for the return type may be returned."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureDimensions ( t: T ) -> vec2<u32>"#,
                doc: r#"Returns the dimensions of a texture, or texture’s mip level in texels.

Returns:

The coordinate dimensions of the texture.

That is, the result provides the integer bounds on the coordinates of the logical texel address, excluding the mip level count, array size, and sample count.

For textures based on cubes, the results are the dimensions of each face of the cube. Cube faces are square, so the x and y components of the result are equal.

If level is outside the range [0, textureNumLevels(t)) then an indeterminate value for the return type may be returned."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureDimensions ( t: T, level: L ) -> vec2<u32>"#,
                doc: r#"Returns the dimensions of a texture, or texture’s mip level in texels.

Returns:

The coordinate dimensions of the texture.

That is, the result provides the integer bounds on the coordinates of the logical texel address, excluding the mip level count, array size, and sample count.

For textures based on cubes, the results are the dimensions of each face of the cube. Cube faces are square, so the x and y components of the result are equal.

If level is outside the range [0, textureNumLevels(t)) then an indeterminate value for the return type may be returned."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureDimensions ( t: T ) -> vec3<u32>"#,
                doc: r#"Returns the dimensions of a texture, or texture’s mip level in texels.

Returns:

The coordinate dimensions of the texture.

That is, the result provides the integer bounds on the coordinates of the logical texel address, excluding the mip level count, array size, and sample count.

For textures based on cubes, the results are the dimensions of each face of the cube. Cube faces are square, so the x and y components of the result are equal.

If level is outside the range [0, textureNumLevels(t)) then an indeterminate value for the return type may be returned."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureDimensions ( t: T, level: L ) -> vec3<u32>"#,
                doc: r#"Returns the dimensions of a texture, or texture’s mip level in texels.

Returns:

The coordinate dimensions of the texture.

That is, the result provides the integer bounds on the coordinates of the logical texel address, excluding the mip level count, array size, and sample count.

For textures based on cubes, the results are the dimensions of each face of the cube. Cube faces are square, so the x and y components of the result are equal.

If level is outside the range [0, textureNumLevels(t)) then an indeterminate value for the return type may be returned."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"textureGather"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( component: C, t: texture_2d<ST>, s: sampler, coords: vec2<f32> ) -> vec4<ST>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( component: C, t: texture_2d<ST>, s: sampler, coords: vec2<f32>, offset: vec2<i32> ) -> vec4<ST>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( component: C, t: texture_2d_array<ST>, s: sampler, coords: vec2<f32>, array_index: A ) -> vec4<ST>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( component: C, t: texture_2d_array<ST>, s: sampler, coords: vec2<f32>, array_index: A, offset: vec2<i32> ) -> vec4<ST>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( component: C, t: texture_cube<ST>, s: sampler, coords: vec3<f32> ) -> vec4<ST>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( component: C, t: texture_cube_array<ST>, s: sampler, coords: vec3<f32>, array_index: A ) -> vec4<ST>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( t: texture_depth_2d, s: sampler, coords: vec2<f32> ) -> vec4<f32>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( t: texture_depth_2d, s: sampler, coords: vec2<f32>, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( t: texture_depth_cube, s: sampler, coords: vec3<f32> ) -> vec4<f32>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( t: texture_depth_2d_array, s: sampler, coords: vec2<f32>, array_index: A ) -> vec4<f32>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( t: texture_depth_2d_array, s: sampler, coords: vec2<f32>, array_index: A, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGather ( t: texture_depth_cube_array, s: sampler, coords: vec3<f32>, array_index: A ) -> vec4<f32>"#,
                doc: r#"A texture gather operation reads from a 2D, 2D array, cube, or cube array texture, computing a four-component vector as follows:

Returns:

A four component vector with components extracted from the specified channel from the selected texels, as described above.

EXAMPLE: Gather components from texels in 2D texture @group ( 0 ) @binding ( 0 ) var t: texture_2d < f32 >; @group ( 0 ) @binding ( 1 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 2 ) var s: sampler; fn gather_x_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 0, t, s, c ); } fn gather_y_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 1, t, s, c ); } fn gather_z_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( 2, t, s, c ); } fn gather_depth_components ( c: vec2 < f32 > ) -> vec4 < f32 > { return textureGather ( dt, s, c ); }"#,
            },
        ],
    },
    BuiltinFn {
        name: r#"textureGatherCompare"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@must_use fn textureGatherCompare ( t: texture_depth_2d, s: sampler_comparison, coords: vec2<f32>, depth_ref: f32 ) -> vec4<f32>"#,
                doc: r#"A texture gather compare operation performs a depth comparison on four texels in a depth texture and collects the results into a single vector, as follows:

Returns:

A four component vector with comparison result for the selected texels, as described above.

EXAMPLE: Gather depth comparison @group ( 0 ) @binding ( 0 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 1 ) var s: sampler; fn gather_depth_compare ( c: vec2 < f32 >, depth_ref: f32 ) -> vec4 < f32 > { return textureGatherCompare ( dt, s, c, depth_ref ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGatherCompare ( t: texture_depth_2d, s: sampler_comparison, coords: vec2<f32>, depth_ref: f32, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"A texture gather compare operation performs a depth comparison on four texels in a depth texture and collects the results into a single vector, as follows:

Returns:

A four component vector with comparison result for the selected texels, as described above.

EXAMPLE: Gather depth comparison @group ( 0 ) @binding ( 0 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 1 ) var s: sampler; fn gather_depth_compare ( c: vec2 < f32 >, depth_ref: f32 ) -> vec4 < f32 > { return textureGatherCompare ( dt, s, c, depth_ref ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGatherCompare ( t: texture_depth_2d_array, s: sampler_comparison, coords: vec2<f32>, array_index: A, depth_ref: f32 ) -> vec4<f32>"#,
                doc: r#"A texture gather compare operation performs a depth comparison on four texels in a depth texture and collects the results into a single vector, as follows:

Returns:

A four component vector with comparison result for the selected texels, as described above.

EXAMPLE: Gather depth comparison @group ( 0 ) @binding ( 0 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 1 ) var s: sampler; fn gather_depth_compare ( c: vec2 < f32 >, depth_ref: f32 ) -> vec4 < f32 > { return textureGatherCompare ( dt, s, c, depth_ref ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGatherCompare ( t: texture_depth_2d_array, s: sampler_comparison, coords: vec2<f32>, array_index: A, depth_ref: f32, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"A texture gather compare operation performs a depth comparison on four texels in a depth texture and collects the results into a single vector, as follows:

Returns:

A four component vector with comparison result for the selected texels, as described above.

EXAMPLE: Gather depth comparison @group ( 0 ) @binding ( 0 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 1 ) var s: sampler; fn gather_depth_compare ( c: vec2 < f32 >, depth_ref: f32 ) -> vec4 < f32 > { return textureGatherCompare ( dt, s, c, depth_ref ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGatherCompare ( t: texture_depth_cube, s: sampler_comparison, coords: vec3<f32>, depth_ref: f32 ) -> vec4<f32>"#,
                doc: r#"A texture gather compare operation performs a depth comparison on four texels in a depth texture and collects the results into a single vector, as follows:

Returns:

A four component vector with comparison result for the selected texels, as described above.

EXAMPLE: Gather depth comparison @group ( 0 ) @binding ( 0 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 1 ) var s: sampler; fn gather_depth_compare ( c: vec2 < f32 >, depth_ref: f32 ) -> vec4 < f32 > { return textureGatherCompare ( dt, s, c, depth_ref ); }"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureGatherCompare ( t: texture_depth_cube_array, s: sampler_comparison, coords: vec3<f32>, array_index: A, depth_ref: f32 ) -> vec4<f32>"#,
                doc: r#"A texture gather compare operation performs a depth comparison on four texels in a depth texture and collects the results into a single vector, as follows:

Returns:

A four component vector with comparison result for the selected texels, as described above.

EXAMPLE: Gather depth comparison @group ( 0 ) @binding ( 0 ) var dt: texture_depth_2d; @group ( 0 ) @binding ( 1 ) var s: sampler; fn gather_depth_compare ( c: vec2 < f32 >, depth_ref: f32 ) -> vec4 < f32 > { return textureGatherCompare ( dt, s, c, depth_ref ); }"#,
            },
        ],
    },
    BuiltinFn {
        name: r#"textureLoad"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_1d<ST>, coords: C, level: L ) -> vec4<ST>"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_2d<ST>, coords: vec2<C>, level: L ) -> vec4<ST>"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_2d_array<ST>, coords: vec2<C>, array_index: A, level: L ) -> vec4<ST>"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_3d<ST>, coords: vec3<C>, level: L ) -> vec4<ST>"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_multisampled_2d<ST>, coords: vec2<C>, sample_index: S )-> vec4<ST>"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_depth_2d, coords: vec2<C>, level: L ) -> f32"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_depth_2d_array, coords: vec2<C>, array_index: A, level: L ) -> f32"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_depth_multisampled_2d, coords: vec2<C>, sample_index: S )-> f32"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_external, coords: vec2<C> ) -> vec4<f32>"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_storage_1d<F, AM>, coords: C ) -> vec4<CF>"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_storage_2d<F, AM>, coords: vec2<C> ) -> vec4<CF>"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_storage_2d_array<F, AM>, coords: vec2<C>, array_index: A ) -> vec4<CF>"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureLoad ( t: texture_storage_3d<F, AM>, coords: vec3<C> ) -> vec4<CF>"#,
                doc: r#"Reads a single texel from a texture without sampling or filtering.

Returns:

The unfiltered texel data.

The logical texel address is invalid if:

any element of coords is outside the range [0, textureDimensions(t, level)) for the corresponding element, or array_index is outside the range [0, textureNumLayers(t)), or level is outside the range [0, textureNumLevels(t)), or sample_index is outside the range [0, textureNumSamples(s))

If the logical texel addresss is invalid, the built-in function returns one of:

The data for some texel within bounds of the texture A vector (0,0,0,0) or (0,0,0,1) of the appropriate type for non-depth textures 0.0 for depth textures"#,
            },
        ],
    },
    BuiltinFn {
        name: r#"textureNumLayers"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn textureNumLayers ( t: T ) -> u32"#,
            doc: r#"Returns the number of layers (elements) of an arrayed texture.

Returns:

If the texture is based on cubes, returns the number of cubes in the cube arrayed texture.

Otherwise returns the number of layers (homogeneous grids of texels) in the arrayed texture."#,
        }],
    },
    BuiltinFn {
        name: r#"textureNumLevels"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn textureNumLevels ( t: T ) -> u32"#,
            doc: r#"Returns the number of mip levels of a texture.

Returns:

The mip level count for the texture."#,
        }],
    },
    BuiltinFn {
        name: r#"textureNumSamples"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn textureNumSamples ( t: T ) -> u32"#,
            doc: r#"Returns the number samples per texel in a multisampled texture.

Returns:

The sample count for the multisampled texture."#,
        }],
    },
    BuiltinFn {
        name: r#"textureSample"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_1d<f32>, s: sampler, coords: f32 ) -> vec4<f32>"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_2d<f32>, s: sampler, coords: vec2<f32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_2d<f32>, s: sampler, coords: vec2<f32>, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_2d_array<f32>, s: sampler, coords: vec2<f32>, array_index: A ) -> vec4<f32>"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_2d_array<f32>, s: sampler, coords: vec2<f32>, array_index: A, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: T, s: sampler, coords: vec3<f32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_3d<f32>, s: sampler, coords: vec3<f32>, offset: vec3<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_cube_array<f32>, s: sampler, coords: vec3<f32>, array_index: A ) -> vec4<f32>"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_depth_2d, s: sampler, coords: vec2<f32> ) -> f32"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_depth_2d, s: sampler, coords: vec2<f32>, offset: vec2<i32> ) -> f32"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_depth_2d_array, s: sampler, coords: vec2<f32>, array_index: A ) -> f32"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_depth_2d_array, s: sampler, coords: vec2<f32>, array_index: A, offset: vec2<i32> ) -> f32"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_depth_cube, s: sampler, coords: vec3<f32> ) -> f32"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSample ( t: texture_depth_cube_array, s: sampler, coords: vec3<f32>, array_index: A ) -> f32"#,
                doc: r#"Samples a texture.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"textureSampleBaseClampToEdge"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn textureSampleBaseClampToEdge ( t: T, s: sampler, coords: vec2<f32> ) -> vec4<f32>"#,
            doc: r#"Samples a texture view at its base level, with texture coordinates clamped to the edge as described below.

Returns:

The sampled value."#,
        }],
    },
    BuiltinFn {
        name: r#"textureSampleBias"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleBias ( t: texture_2d<f32>, s: sampler, coords: vec2<f32>, bias: f32 ) -> vec4<f32>"#,
                doc: r#"Samples a texture with a bias to the mip level.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleBias ( t: texture_2d<f32>, s: sampler, coords: vec2<f32>, bias: f32, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture with a bias to the mip level.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleBias ( t: texture_2d_array<f32>, s: sampler, coords: vec2<f32>, array_index: A, bias: f32 ) -> vec4<f32>"#,
                doc: r#"Samples a texture with a bias to the mip level.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleBias ( t: texture_2d_array<f32>, s: sampler, coords: vec2<f32>, array_index: A, bias: f32, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture with a bias to the mip level.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleBias ( t: T, s: sampler, coords: vec3<f32>, bias: f32 ) -> vec4<f32>"#,
                doc: r#"Samples a texture with a bias to the mip level.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleBias ( t: texture_3d<f32>, s: sampler, coords: vec3<f32>, bias: f32, offset: vec3<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture with a bias to the mip level.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleBias ( t: texture_cube_array<f32>, s: sampler, coords: vec3<f32>, array_index: A, bias: f32 ) -> vec4<f32>"#,
                doc: r#"Samples a texture with a bias to the mip level.

Returns:

The sampled value.

An indeterminate value results if called in non-uniform control flow."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"textureSampleCompare"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompare ( t: texture_depth_2d, s: sampler_comparison, coords: vec2<f32>, depth_ref: f32 ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

Each sampled texel is compared against the reference value using the comparison operator defined by the sampler_comparison, resulting in either a 0 or 1 value for each texel.

If the sampler uses bilinear filtering then the returned value is the filtered average of these values, otherwise the comparison result of a single texel is returned.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompare ( t: texture_depth_2d, s: sampler_comparison, coords: vec2<f32>, depth_ref: f32, offset: vec2<i32> ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

Each sampled texel is compared against the reference value using the comparison operator defined by the sampler_comparison, resulting in either a 0 or 1 value for each texel.

If the sampler uses bilinear filtering then the returned value is the filtered average of these values, otherwise the comparison result of a single texel is returned.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompare ( t: texture_depth_2d_array, s: sampler_comparison, coords: vec2<f32>, array_index: A, depth_ref: f32 ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

Each sampled texel is compared against the reference value using the comparison operator defined by the sampler_comparison, resulting in either a 0 or 1 value for each texel.

If the sampler uses bilinear filtering then the returned value is the filtered average of these values, otherwise the comparison result of a single texel is returned.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompare ( t: texture_depth_2d_array, s: sampler_comparison, coords: vec2<f32>, array_index: A, depth_ref: f32, offset: vec2<i32> ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

Each sampled texel is compared against the reference value using the comparison operator defined by the sampler_comparison, resulting in either a 0 or 1 value for each texel.

If the sampler uses bilinear filtering then the returned value is the filtered average of these values, otherwise the comparison result of a single texel is returned.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompare ( t: texture_depth_cube, s: sampler_comparison, coords: vec3<f32>, depth_ref: f32 ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

Each sampled texel is compared against the reference value using the comparison operator defined by the sampler_comparison, resulting in either a 0 or 1 value for each texel.

If the sampler uses bilinear filtering then the returned value is the filtered average of these values, otherwise the comparison result of a single texel is returned.

An indeterminate value results if called in non-uniform control flow."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompare ( t: texture_depth_cube_array, s: sampler_comparison, coords: vec3<f32>, array_index: A, depth_ref: f32 ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

Each sampled texel is compared against the reference value using the comparison operator defined by the sampler_comparison, resulting in either a 0 or 1 value for each texel.

If the sampler uses bilinear filtering then the returned value is the filtered average of these values, otherwise the comparison result of a single texel is returned.

An indeterminate value results if called in non-uniform control flow."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"textureSampleCompareLevel"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompareLevel ( t: texture_depth_2d, s: sampler_comparison, coords: vec2<f32>, depth_ref: f32 ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

The textureSampleCompareLevel function is the same as textureSampleCompare, except that:

textureSampleCompareLevel always samples texels from mip level 0. The function does not compute derivatives. There is no requirement for textureSampleCompareLevel to be invoked in uniform control flow. textureSampleCompareLevel may be invoked in any shader stage."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompareLevel ( t: texture_depth_2d, s: sampler_comparison, coords: vec2<f32>, depth_ref: f32, offset: vec2<i32> ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

The textureSampleCompareLevel function is the same as textureSampleCompare, except that:

textureSampleCompareLevel always samples texels from mip level 0. The function does not compute derivatives. There is no requirement for textureSampleCompareLevel to be invoked in uniform control flow. textureSampleCompareLevel may be invoked in any shader stage."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompareLevel ( t: texture_depth_2d_array, s: sampler_comparison, coords: vec2<f32>, array_index: A, depth_ref: f32 ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

The textureSampleCompareLevel function is the same as textureSampleCompare, except that:

textureSampleCompareLevel always samples texels from mip level 0. The function does not compute derivatives. There is no requirement for textureSampleCompareLevel to be invoked in uniform control flow. textureSampleCompareLevel may be invoked in any shader stage."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompareLevel ( t: texture_depth_2d_array, s: sampler_comparison, coords: vec2<f32>, array_index: A, depth_ref: f32, offset: vec2<i32> ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

The textureSampleCompareLevel function is the same as textureSampleCompare, except that:

textureSampleCompareLevel always samples texels from mip level 0. The function does not compute derivatives. There is no requirement for textureSampleCompareLevel to be invoked in uniform control flow. textureSampleCompareLevel may be invoked in any shader stage."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompareLevel ( t: texture_depth_cube, s: sampler_comparison, coords: vec3<f32>, depth_ref: f32 ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

The textureSampleCompareLevel function is the same as textureSampleCompare, except that:

textureSampleCompareLevel always samples texels from mip level 0. The function does not compute derivatives. There is no requirement for textureSampleCompareLevel to be invoked in uniform control flow. textureSampleCompareLevel may be invoked in any shader stage."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleCompareLevel ( t: texture_depth_cube_array, s: sampler_comparison, coords: vec3<f32>, array_index: A, depth_ref: f32 ) -> f32"#,
                doc: r#"Samples a depth texture and compares the sampled depth values against a reference value.

Returns:

A value in the range [0.0..1.0].

The textureSampleCompareLevel function is the same as textureSampleCompare, except that:

textureSampleCompareLevel always samples texels from mip level 0. The function does not compute derivatives. There is no requirement for textureSampleCompareLevel to be invoked in uniform control flow. textureSampleCompareLevel may be invoked in any shader stage."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"textureSampleGrad"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleGrad ( t: texture_2d<f32>, s: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture using explicit gradients.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleGrad ( t: texture_2d<f32>, s: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture using explicit gradients.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleGrad ( t: texture_2d_array<f32>, s: sampler, coords: vec2<f32>, array_index: A, ddx: vec2<f32>, ddy: vec2<f32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture using explicit gradients.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleGrad ( t: texture_2d_array<f32>, s: sampler, coords: vec2<f32>, array_index: A, ddx: vec2<f32>, ddy: vec2<f32>, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture using explicit gradients.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleGrad ( t: T, s: sampler, coords: vec3<f32>, ddx: vec3<f32>, ddy: vec3<f32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture using explicit gradients.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleGrad ( t: texture_3d<f32>, s: sampler, coords: vec3<f32>, ddx: vec3<f32>, ddy: vec3<f32>, offset: vec3<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture using explicit gradients.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleGrad ( t: texture_cube_array<f32>, s: sampler, coords: vec3<f32>, array_index: A, ddx: vec3<f32>, ddy: vec3<f32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture using explicit gradients.

Returns:

The sampled value."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"textureSampleLevel"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_2d<f32>, s: sampler, coords: vec2<f32>, level: f32 ) -> vec4<f32>"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_2d<f32>, s: sampler, coords: vec2<f32>, level: f32, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_2d_array<f32>, s: sampler, coords: vec2<f32>, array_index: A, level: f32 ) -> vec4<f32>"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_2d_array<f32>, s: sampler, coords: vec2<f32>, array_index: A, level: f32, offset: vec2<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: T, s: sampler, coords: vec3<f32>, level: f32 ) -> vec4<f32>"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_3d<f32>, s: sampler, coords: vec3<f32>, level: f32, offset: vec3<i32> ) -> vec4<f32>"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_cube_array<f32>, s: sampler, coords: vec3<f32>, array_index: A, level: f32 ) -> vec4<f32>"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_depth_2d, s: sampler, coords: vec2<f32>, level: L ) -> f32"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_depth_2d, s: sampler, coords: vec2<f32>, level: L, offset: vec2<i32> ) -> f32"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_depth_2d_array, s: sampler, coords: vec2<f32>, array_index: A, level: L ) -> f32"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_depth_2d_array, s: sampler, coords: vec2<f32>, array_index: A, level: L, offset: vec2<i32> ) -> f32"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_depth_cube, s: sampler, coords: vec3<f32>, level: L ) -> f32"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
            BuiltinOverload {
                signature: r#"@must_use fn textureSampleLevel ( t: texture_depth_cube_array, s: sampler, coords: vec3<f32>, array_index: A, level: L ) -> f32"#,
                doc: r#"Samples a texture using an explicit mip level.

Returns:

The sampled value."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"textureStore"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"fn textureStore ( t: texture_storage_1d<F, AM>, coords: C, value: vec4<CF> )"#,
                doc: r#"Writes a single texel to a texture."#,
            },
            BuiltinOverload {
                signature: r#"fn textureStore ( t: texture_storage_2d<F, AM>, coords: vec2<C>, value: vec4<CF> )"#,
                doc: r#"Writes a single texel to a texture."#,
            },
            BuiltinOverload {
                signature: r#"fn textureStore ( t: texture_storage_2d_array<F, AM>, coords: vec2<C>, array_index: A, value: vec4<CF> )"#,
                doc: r#"Writes a single texel to a texture."#,
            },
            BuiltinOverload {
                signature: r#"fn textureStore ( t: texture_storage_3d<F, AM>, coords: vec3<C>, value: vec4<CF> )"#,
                doc: r#"Writes a single texel to a texture."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"transpose"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn transpose ( e: matRxC<T> ) -> matCxR<T>"#,
            doc: r#"Returns the transpose of e."#,
        }],
    },
    BuiltinFn {
        name: r#"trunc"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn trunc ( e: T ) -> T"#,
            doc: r#"Returns truncate ( e ), the nearest whole number whose absolute value is less than or equal to the absolute value of e. Component-wise when T is a vector."#,
        }],
    },
    BuiltinFn {
        name: r#"u32"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn u32 ( e: T ) -> u32"#,
            doc: r#"Construct a u32 value. If T is u32, this is an identity operation. If T is i32, this is a reinterpretation of bits (i.e. the result is the unique value in u32 that has the same bit pattern as e ). If T is a floating point type, e is converted to u32, rounding towards zero. If T is bool, the result is 1u if e is true and 0u otherwise. If T is AbstractInt, this is an identity operation if the e can be represented in u32, otherwise it produces a shader-creation error."#,
        }],
    },
    BuiltinFn {
        name: r#"unpack2x16float"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn unpack2x16float ( e: u32 ) -> vec2<f32>"#,
            doc: r#"Decomposes a 32-bit value into two 16-bit chunks, and reinterpets each chunk as a floating point value. Component i of the result is the f32 representation of v, where v is the interpretation of bits 16× i through 16× i + 15 of e as an IEEE-754 binary16 value. See § 14.6.4 Floating Point Conversion."#,
        }],
    },
    BuiltinFn {
        name: r#"unpack2x16snorm"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn unpack2x16snorm ( e: u32 ) -> vec2<f32>"#,
            doc: r#"Decomposes a 32-bit value into two 16-bit chunks, then reinterprets each chunk as a signed normalized floating point value. Component i of the result is max(v ÷ 32767, -1), where v is the interpretation of bits 16× i through 16× i + 15 of e as a twos-complement signed integer."#,
        }],
    },
    BuiltinFn {
        name: r#"unpack2x16unorm"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn unpack2x16unorm ( e: u32 ) -> vec2<f32>"#,
            doc: r#"Decomposes a 32-bit value into two 16-bit chunks, then reinterprets each chunk as an unsigned normalized floating point value. Component i of the result is v ÷ 65535, where v is the interpretation of bits 16× i through 16× i + 15 of e as an unsigned integer."#,
        }],
    },
    BuiltinFn {
        name: r#"unpack4x8snorm"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn unpack4x8snorm ( e: u32 ) -> vec4<f32>"#,
            doc: r#"Decomposes a 32-bit value into four 8-bit chunks, then reinterprets each chunk as a signed normalized floating point value. Component i of the result is max(v ÷ 127, -1), where v is the interpretation of bits 8× i through 8× i + 7 of e as a twos-complement signed integer."#,
        }],
    },
    BuiltinFn {
        name: r#"unpack4x8unorm"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn unpack4x8unorm ( e: u32 ) -> vec4<f32>"#,
            doc: r#"Decomposes a 32-bit value into four 8-bit chunks, then reinterprets each chunk as an unsigned normalized floating point value. Component i of the result is v ÷ 255, where v is the interpretation of bits 8× i through 8× i + 7 of e as an unsigned integer."#,
        }],
    },
    BuiltinFn {
        name: r#"unpack4xI8"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn unpack4xI8 ( e: u32 ) -> vec4<i32>"#,
            doc: r#"e is interpreted as a vector with four 8-bit signed integer components. Unpack e into a vec4<i32> with sign extension."#,
        }],
    },
    BuiltinFn {
        name: r#"unpack4xU8"#,
        overloads: &[BuiltinOverload {
            signature: r#"@const @must_use fn unpack4xU8 ( e: u32 ) -> vec4<u32>"#,
            doc: r#"e is interpreted as a vector with four 8-bit unsigned integer components. Unpack e into a vec4<u32> with zero extension."#,
        }],
    },
    BuiltinFn {
        name: r#"vec2"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn vec2<T> ( e: T ) -> vec2<T> @const @must_use fn vec2 ( e: S ) -> vec2<S>"#,
                doc: r#"Construction of a two-component vector with e as both components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec2<T> ( e: vec2<S> ) -> vec2<T> @const @must_use fn vec2 ( e: vec2<S> ) -> vec2<S>"#,
                doc: r#"Component-wise construction of a two-component vector with e.x and e.y as components. If T does not match S a conversion is used and the components are T(e.x) and T(e.y)."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec2<T> ( e1: T, e2: T ) -> vec2<T> @const @must_use fn vec2 ( e1: T, e2: T ) -> vec2<T>"#,
                doc: r#"Component-wise construction of a two-component vector with e1 and e2 as components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec2 () -> vec2<T>"#,
                doc: r#"Returns the value vec2(0,0)."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"vec3"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn vec3<T> ( e: T ) -> vec3<T> @const @must_use fn vec3 ( e: S ) -> vec3<S>"#,
                doc: r#"Construction of a three-component vector with e as all components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec3<T> ( e: vec3<S> ) -> vec3<T> @const @must_use fn vec3 ( e: vec3<S> ) -> vec3<S>"#,
                doc: r#"Component-wise construction of a three-component vector with e.x, e.y, and e.z as components. If T does not match S a conversion is used and the components are T(e.x), T(e.y), and T(e.z)."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec3<T> ( e1: T, e2: T, e3: T ) -> vec3<T> @const @must_use fn vec3 ( e1: T, e2: T, e3: T ) -> vec3<T>"#,
                doc: r#"Component-wise construction of a three-component vector with e1, e2, and e3 as components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec3<T> ( v1: vec2<T>, e1: T ) -> vec3<T> @const @must_use fn vec3 ( v1: vec2<T>, e1: T ) -> vec3<T>"#,
                doc: r#"Component-wise construction of a three-component vector with v1.x, v1.y, and e1 as components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec3<T> ( e1: T, v1: vec2<T> ) -> vec3<T> @const @must_use fn vec3 ( e1: T, v1: vec2<T> ) -> vec3<T>"#,
                doc: r#"Component-wise construction of a three-component vector with e1, v1.x, and v1.y as components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec3 () -> vec3<T>"#,
                doc: r#"Returns the value vec3(0,0,0)."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"vec4"#,
        overloads: &[
            BuiltinOverload {
                signature: r#"@const @must_use fn vec4<T> ( e: T ) -> vec4<T> @const @must_use fn vec4 ( e: S ) -> vec4<S>"#,
                doc: r#"Construction of a four-component vector with e as all components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec4<T> ( e: vec4<S> ) -> vec4<T> @const @must_use fn vec4 ( e: vec4<S> ) -> vec4<S>"#,
                doc: r#"Component-wise construction of a four-component vector with e.x, e.y, e.z, and e.w as components. If T does not match S a conversion is used and the components are T(e.x), T(e.y), T(e.z) and T(e.w)."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec4<T> ( e1: T, e2: T, e3: T, e4: T ) -> vec4<T> @const @must_use fn vec4 ( e1: T, e2: T, e3: T, e4: T ) -> vec4<T>"#,
                doc: r#"Component-wise construction of a four-component vector with e1, e2, e3, and e4 as components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec4<T> ( e1: T, v1: vec2<T>, e2: T ) -> vec4<T> @const @must_use fn vec4 ( e1: T, v1: vec2<T>, e2: T ) -> vec4<T>"#,
                doc: r#"Component-wise construction of a four-component vector with e1, v1.x, v1.y, and e2 as components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec4<T> ( e1: T, e2: T, v1: vec2<T> ) -> vec4<T> @const @must_use fn vec4 ( e1: T, e2: T, v1: vec2<T> ) -> vec4<T>"#,
                doc: r#"Component-wise construction of a four-component vector with e1, e2, v1.x, and v1.y as components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec4<T> ( v1: vec2<T>, v2: vec2<T> ) -> vec4<T> @const @must_use fn vec4 ( v1: vec2<T>, v2: vec2<T> ) -> vec4<T>"#,
                doc: r#"Component-wise construction of a four-component vector with v1.x, v1.y, v2.x, and v2.y as components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec4<T> ( v1: vec2<T>, e1: T, e2: T ) -> vec4<T> @const @must_use fn vec4 ( v1: vec2<T>, e1: T, e2: T ) -> vec4<T>"#,
                doc: r#"Component-wise construction of a four-component vector with v1.x, v1.y, e1, and e2 as components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec4<T> ( v1: vec3<T>, e1: T ) -> vec4<T> @const @must_use fn vec4 ( v1: vec3<T>, e1: T ) -> vec4<T>"#,
                doc: r#"Component-wise construction of a four-component vector with v1.x, v1.y, v1.z, and e1 as components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec4<T> ( e1: T, v1: vec3<T> ) -> vec4<T> @const @must_use fn vec4 ( e1: T, v1: vec3<T> ) -> vec4<T>"#,
                doc: r#"Component-wise construction of a four-component vector with e1, v1.x, v1.y, and v1.z as components."#,
            },
            BuiltinOverload {
                signature: r#"@const @must_use fn vec4 () -> vec4<T>"#,
                doc: r#"Returns the value vec4(0,0,0,0)."#,
            },
        ],
    },
    BuiltinFn {
        name: r#"workgroupBarrier"#,
        overloads: &[BuiltinOverload {
            signature: r#"fn workgroupBarrier ()"#,
            doc: r#"Executes a control barrier synchronization function that affects memory and atomic operations in the workgroup address space."#,
        }],
    },
    BuiltinFn {
        name: r#"workgroupUniformLoad"#,
        overloads: &[BuiltinOverload {
            signature: r#"@must_use fn workgroupUniformLoad ( p: ptr<workgroup, T> ) -> T"#,
            doc: r#"Returns the value pointed to by p to all invocations in the workgroup. The return value is uniform. p must be a uniform value. Executes a control barrier synchronization function that affects memory and atomic operations in the workgroup address space."#,
        }],
    },
];

pub fn builtin(name: &str) -> Option<&'static BuiltinFn> {
    BUILTIN_FUNCTIONS
        .binary_search_by_key(&name, |builtin| builtin.name)
        .ok()
        .map(|index| &BUILTIN_FUNCTIONS[index])
}

pub static BUILTIN_TYPES: &[&str] = &[
    "array",
    "atomic",
    "bool",
    "f16",
    "f32",
    "i32",
    "mat2x2",
    "mat2x3",
    "mat2x4",
    "mat3x2",
    "mat3x3",
    "mat3x4",
    "mat4x2",
    "mat4x3",
    "mat4x4",
    "ptr",
    "sampler",
    "sampler_comparison",
    "texture_1d",
    "texture_2d",
    "texture_2d_array",
    "texture_3d",
    "texture_cube",
    "texture_cube_array",
    "texture_depth_2d",
    "texture_depth_2d_array",
    "texture_depth_cube",
    "texture_depth_cube_array",
    "texture_external",
    "texture_multisampled_2d",
    "texture_storage_1d",
    "texture_storage_2d",
    "texture_storage_2d_array",
    "texture_storage_3d",
    "u32",
    "vec2",
    "vec3",
    "vec4",
];
