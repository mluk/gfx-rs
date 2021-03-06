// Copyright 2014 The Gfx-rs Developers.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Shader parameter handling.

use std::cell::RefCell;
use device::shade;
use device::shade::UniformValue;
use device::{handle, Resources};
use super::ParamStorage;

pub use device::shade::{Stage, CreateShaderError};


macro_rules! uniform {
    ($ty_src:ty, $ty_dst:ident) => {
        impl Into<UniformValue> for $ty_src {
            fn into(self) -> UniformValue {
                UniformValue::$ty_dst(self)
            }
        }
    }
}

uniform!(i32, I32);
uniform!(f32, F32);

uniform!([i32; 2], I32Vector2);
uniform!([i32; 3], I32Vector3);
uniform!([i32; 4], I32Vector4);

uniform!([f32; 2], F32Vector2);
uniform!([f32; 3], F32Vector3);
uniform!([f32; 4], F32Vector4);

uniform!([[f32; 2]; 2], F32Matrix2);
uniform!([[f32; 3]; 3], F32Matrix3);
uniform!([[f32; 4]; 4], F32Matrix4);

/// A texture parameter: consists of a texture handle with an optional sampler.
/// Not all textures need a sampler (i.e. MSAA ones do not). Optimally, we'd want to
/// encode this logic into the type system (TODO).
pub type TextureParam<R: Resources> = (handle::Texture<R>, Option<handle::Sampler<R>>);

/// An error type on either the parameter storage or the program side
#[derive(Clone, PartialEq, Debug)]
pub enum ParameterError {
    /// The parameter requires 'self' to be assigned, but none was provided.
    MissingSelf,
    /// Shader requested a uniform that the parameters do not have.
    MissingUniform(String),
    /// Shader requested a uniform that the parameters do not match.
    BadUniform(String),
    /// Shader requested a block that the parameters do not have.
    MissingBlock(String),
    /// Shader requested a block that the parameters do not match.
    BadBlock(String),
    /// Shader requested a texture that the parameters do not have.
    MissingTexture(String),
    /// Shader requested a texture that the parameters do not match.
    BadTexture(String),
}

/// Parameter index.
pub type ParameterId = u16;

/// General shader parameter.
pub trait Parameter<R: Resources> {
    /// Check if this parameter is good for a given uniform.
    fn check_uniform(&shade::UniformVar) -> bool { false }
    /// Check if this parameter is good for a given block.
    fn check_block(&shade::BlockVar) -> bool { false }
    /// Check if this parameter is good for a given texture.
    fn check_texture(&shade::SamplerVar) -> bool { false }
    /// Write into the parameter storage for rendering.
    fn put(&self, ParameterId, &mut ParamStorage<R>);
}

impl<T: Clone + Into<UniformValue>, R: Resources> Parameter<R> for T {
    fn check_uniform(_var: &shade::UniformVar) -> bool {
        true //TODO
    }

    fn put(&self, id: ParameterId, storage: &mut ParamStorage<R>) {
        storage.uniforms[id as usize] = Some(self.clone().into());
    }
}

impl<R: Resources> Parameter<R> for handle::RawBuffer<R> {
    fn check_block(_var: &shade::BlockVar) -> bool {
        true
    }

    fn put(&self, id: ParameterId, storage: &mut ParamStorage<R>) {
        storage.blocks[id as usize] = Some(self.clone());
    }
}

impl<R: Resources> Parameter<R> for TextureParam<R> {
    fn check_texture(_var: &shade::SamplerVar) -> bool {
        true
    }

    fn put(&self, id: ParameterId, storage: &mut ParamStorage<R>) {
        storage.textures[id as usize] = Some(self.clone());
    }
}


/// Abstracts the shader parameter structure, generated by the `shader_param` attribute
#[allow(missing_docs)]
pub trait ShaderParam {
    type Resources: Resources;
    /// A helper structure to contain variable indices inside the shader
    type Link: Clone;
    /// Create a new link to be used with a given program
    fn create_link(Option<&Self>, &shade::ProgramInfo) -> Result<Self::Link, ParameterError>;
    /// Get all the contained parameter values, using a given link
    fn fill_params(&self, &Self::Link, &mut ParamStorage<Self::Resources>);
}

impl<R: Resources> ShaderParam for Option<R> {
    type Resources = R;
    type Link = ();

    fn create_link(_: Option<&Option<R>>, info: &shade::ProgramInfo) -> Result<(), ParameterError> {
        match info.uniforms[..].first() {
            Some(u) => return Err(ParameterError::MissingUniform(u.name.clone())),
            None => (),
        }
        match info.blocks[..].first() {
            Some(b) => return Err(ParameterError::MissingBlock(b.name.clone())),
            None => (),
        }
        match info.textures[..].first() {
            Some(t) => return Err(ParameterError::MissingTexture(t.name.clone())),
            None => (),
        }
        Ok(())
    }

    fn fill_params(&self, _: &(), _: &mut ParamStorage<R>) {
        //empty
    }
}

/// A named cell containing arbitrary value
pub struct NamedCell<T> {
    /// Name
    pub name: String,
    /// Value
    pub value: RefCell<T>,
}

/// A dictionary of parameters, meant to be shared between different programs
pub struct ParamDictionary<R: Resources> {
    /// Uniform dictionary
    pub uniforms: Vec<NamedCell<shade::UniformValue>>,
    /// Block dictionary
    pub blocks: Vec<NamedCell<handle::RawBuffer<R>>>,
    /// Texture dictionary
    pub textures: Vec<NamedCell<TextureParam<R>>>,
}

/// Redirects program input to the relevant ParamDictionary cell
#[derive(Clone)]
pub struct ParamDictionaryLink {
    uniforms: Vec<usize>,
    blocks: Vec<usize>,
    textures: Vec<usize>,
}

impl<R: Resources> ShaderParam for ParamDictionary<R> {
    type Resources = R;
    type Link = ParamDictionaryLink;

    fn create_link(this: Option<&ParamDictionary<R>>, info: &shade::ProgramInfo)
                   -> Result<ParamDictionaryLink, ParameterError> {
        let this = match this {
            Some(d) => d,
            None => return Err(ParameterError::MissingSelf),
        };
        //TODO: proper error checks
        Ok(ParamDictionaryLink {
            uniforms: info.uniforms.iter().map(|var|
                this.uniforms.iter().position(|c| c.name == var.name).unwrap()
            ).collect(),
            blocks: info.blocks.iter().map(|var|
                this.blocks  .iter().position(|c| c.name == var.name).unwrap()
            ).collect(),
            textures: info.textures.iter().map(|var|
                this.textures.iter().position(|c| c.name == var.name).unwrap()
            ).collect(),
        })
    }

    fn fill_params(&self, link: &ParamDictionaryLink, params: &mut ParamStorage<R>) {
        for &id in link.uniforms.iter() {
            params.uniforms[id] = Some(self.uniforms[id].value.borrow().clone());
        }
        for &id in link.blocks.iter() {
            params.blocks[id] = Some(self.blocks[id].value.borrow().clone());
        }
        for &id in link.textures.iter() {
            params.textures[id] = Some(self.textures[id].value.borrow().clone());
        }
    }
}
