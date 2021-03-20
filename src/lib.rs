extern crate tokio;
use std::{ffi::{CStr, c_void}, io::ErrorKind,  time::Duration};
use bytes::BufMut;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, runtime::Runtime,  time::{ interval}};
use tokio::net::TcpStream;
use tokio::io::Interest;
use tokio::select;
#[cfg(test)]
mod tests {
    use std::{ffi::{CString}};

    #[test]
    fn it_works() {
        use super::*;
        
        let s = CString::new("localhost:8080").unwrap();
        
        let client = Client_New(s.as_ptr());
        assert!(client != 0 as *mut Client);
        Client_OnMessage(client, on_message, 0 as *mut c_void);
        std::thread::sleep(Duration::from_secs(5));
        Client_Delete(client);

    }
}
extern "C" fn on_message(_:*mut Client, msg:*const i8, _:*mut std::ffi::c_void){
    let msg = unsafe{
        CStr::from_ptr(msg)
    };
    let msg = match msg.to_str(){
        Ok(v)=>v,
        Err(_)=>return
    };
    println!("{}", msg);
}
pub struct Client{
    is_running:bool,
    message_queue:tokio::sync::mpsc::Sender<String>,
    runtime:Option<Runtime>,
    stream:TcpStream,
    message_handler:Option<(usize, MessageHandler)>
}
const MSG_TYPE_HELLO:[u8;4] = *b"HELO"; //i32::from_be_bytes(*b"HELO"); 
const MSG_TYPE_CHAT:[u8;4] = *b"CHAT"; //i32::from_be_bytes(*b"CHAT"); 
const MSG_TYPE_PING:[u8;4] = *b"PING"; //i32::from_be_bytes(*); 
#[no_mangle]
pub extern "C" fn Client_New(url:*const i8)->*mut Client {
    let url = unsafe{
        CStr::from_ptr(url)
    };
    let url = match url.to_str(){
        Ok(v)=>v.to_string(),
        Err(_)=>return 0 as *mut _   
    };
    match Client::new(url){
        Ok(v)=>Box::into_raw(v),
        Err( _ )=>std::ptr::null_mut()
    }
}
type MessageHandler = extern "C" fn(*mut Client, *const i8, *mut std::ffi::c_void)->();
#[no_mangle]
pub extern "C" fn Client_OnMessage(client:*mut Client, handler:MessageHandler, handle_data:*mut std::ffi::c_void) {
    let obj =unsafe{ client.as_mut()};
    let obj = match obj{
        Some(obj)=>obj,
        None=>{
            return;
        }
    };
    obj.message_handler = Some((
        handle_data as usize,
        handler
    ));
}
#[no_mangle]
pub extern "C" fn Client_Delete(client:*mut Client) {
    if client.is_null(){
        return;
    }
    std::mem::drop(unsafe{
        Box::from_raw(client);
    });
}
#[no_mangle]
pub extern "C" fn Client_SendMessage(client:*mut Client,msg: *const i8){
    let obj =unsafe{ client.as_mut()};
    let obj = match obj{
        Some(obj)=>obj,
        None=>{
            return;
        }
    };
    let msg = unsafe{
        CStr::from_ptr(msg)
    };
    let msg = match msg.to_str(){
        Ok(v)=>v.to_string(),
        Err(_)=>return
    };
    let task = obj.send_message(msg);
    let _ = obj.runtime.as_ref().unwrap().block_on(task);
}

impl Client{
    pub fn new(url:String)->Result<Box<Self>, ()>{
        let runtime = match Runtime::new(){
            Ok(v)=>v,
            Err(_)=>return Err(())
        };
        let s = runtime.spawn(async move{
            let mut stream = match TcpStream::connect(url).await{
                Ok(stream)=>stream,
                Err(_)=>return Err( () )
            };
            if let Err(e) = stream.write_all(&MSG_TYPE_HELLO).await{
                eprintln!("{:?}", e);
                return Err( () );
            }
            let mut s = [0u8;4];
            if let Err(e) = stream.read_exact(&mut s).await{
                eprintln!("{:?}", e);
                return Err( () );
            }
            return Ok(stream);
        });
        
        if let Ok(connect_res) = runtime.block_on(s){
            if let Err( _ ) = connect_res{
                return Err(())
            }
            let stream =connect_res.unwrap();
            let (tx, rx) = tokio::sync::mpsc::channel(1024);
            let obj = Box::new(Client{
                is_running:true,
                message_queue:tx,
                runtime:Some(runtime),
                stream,
                message_handler:None
            });
            let mut obj =obj;
            let ptr:*mut Client = obj.as_mut();
            let obj_ref =obj.as_mut();
            let obj_ref2 =unsafe{ ptr.as_mut().unwrap()};
            let obj_ref3 =unsafe{ ptr.as_mut().unwrap()};
            obj_ref.runtime.as_mut().unwrap().spawn(obj_ref2.write_process(rx));
            obj_ref.runtime.as_mut().unwrap().spawn(obj_ref3.process());
            return Ok(obj);
        }
        return Err(());
    }
    pub async fn send_message(&self, msg:String)->std::io::Result<()>{
        if let Err(_)= self.message_queue.send(msg).await{
            return Err(std::io::Error::new(std::io::ErrorKind::Other, ""));
        }
        return Ok(());
    }
    pub async fn write_process(&self, mut rx:tokio::sync::mpsc::Receiver<String>)->std::io::Result<()>{
        let mut timeout = interval(Duration::new(1,0));
        let mut bytes:Vec<u8> = Vec::new();
        
        while self.is_running{
            let recv_task = rx.recv();
            let timeout_task = timeout.tick();

            tokio::pin!(timeout_task);
            tokio::pin!(recv_task);
            select!{
                val=recv_task=>{
                    if let Some(msg)=val{
                        println!("{}",msg);
                        let str_len = msg.len();
                        bytes.put_slice(&MSG_TYPE_CHAT);
                        bytes.put_slice(&u32::to_be_bytes(str_len as u32));
                        bytes.put_slice(msg.as_bytes());
                    }
                },
                _=&mut timeout_task=>{
                    bytes.put_slice(&MSG_TYPE_PING);
                }
            }
            let ready = self.stream.ready(Interest::WRITABLE).await?;
            if ready.is_writable(){
                let write_count = self.stream.try_write(bytes.as_slice())?;
                bytes.drain(..write_count);
            }
        }
        return Ok(());
    }
    pub async fn process(&mut self)
    ->std::io::Result<()>{
        let mut rest : Vec<u8> = Vec::new();
        let mut buffer = [0u8;1024];
        while self.is_running{
            let ready = self.stream.ready(Interest::READABLE).await?;
            if ready.is_readable(){
                let data_size = match self.stream.try_read(&mut buffer[..]){
                    Ok(size)=>size,
                    Err(e)=>if let ErrorKind::WouldBlock = e.kind(){
                        continue;
                    }
                    else{
                        return Err(e);
                    }
                };
                if data_size == 0{
                    continue;
                }
                rest.put_slice(&buffer[0..data_size]);
                let buffer_size = rest.len();
                let mut last_buffer_size = 0;
                for i in 0..buffer_size{
                    let ch = rest[i];
                    if ch == 0u8{
                        let start = last_buffer_size;
                        last_buffer_size = i + 1;
                        let (data, handler) = match self.message_handler{
                            Some(v)=>v,
                            None=>continue
                        };
                        handler(self as *mut Client, rest[start..i].as_ptr()  as *const i8, data as *mut c_void);
                    }
                }
                rest.drain(..last_buffer_size);
            }
        }
        
        return Ok(());
    }
}
impl Drop for Client{
    fn drop(&mut self){
        self.is_running = false; 
        let mut runtime = match std::mem::replace(&mut self.runtime, None){
            Some(v)=>v,
            None=>return
        };
    }
}