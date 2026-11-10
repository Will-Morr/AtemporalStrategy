import type {Game} from './game';

/** Private coordinator feedback, stored next to this exact match's replay ledger. */
export function feedbackForm(g:Game):HTMLElement {
  const root=document.createElement('div');
  const open=document.createElement('button');open.textContent='Give feedback';
  const form=document.createElement('form');form.id='feedback-form';form.hidden=true;
  const label=document.createElement('label');label.htmlFor='feedback-message';label.textContent='Message to the creator';
  const message=document.createElement('textarea');message.id='feedback-message';message.maxLength=5000;message.required=true;
  const note=document.createElement('p');note.textContent='Only the creator can read this. Your message will include a reference to this game.';
  const submit=document.createElement('button');submit.type='submit';submit.textContent='Send feedback';
  const status=document.createElement('p');status.id='feedback-status';status.setAttribute('role','status');
  const cancel=document.createElement('button');cancel.type='button';cancel.textContent='Close feedback';cancel.onclick=()=>{form.hidden=true;open.hidden=false;};
  open.onclick=()=>{form.hidden=false;open.hidden=true;message.focus();};
  let submission:string|null=null,lastMessage='';
  const match=g.net.matchId; // Capture this match even if the directory later hosts a rematch.
  form.onsubmit=async event=>{
    event.preventDefault();if(submit.disabled||!match)return;
    if(!message.value.trim()){status.textContent='Please write a message first.';return;}
    if(!submission||lastMessage!==message.value){
      submission=Array.from(crypto.getRandomValues(new Uint8Array(16)),b=>b.toString(16).padStart(2,'0')).join('');lastMessage=message.value;
    }
    submit.disabled=true;status.textContent='Saving your feedback…';
    try{
      const id=location.pathname.split('/')[2];
      const response=await fetch(`/api/games/${id}/feedback`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({match_id:match,revision:g.current,submission_id:submission,message:message.value})});
      if(!response.ok)throw new Error('Could not save feedback. Your message is still here; please retry.');
      const result=await response.json();if(!result.saved)throw new Error(result.error??'Could not save feedback. Please retry.');
      status.textContent='Thank you — your feedback has been saved for the creator.';message.readOnly=true;
    }catch(error){status.textContent=(error as Error).message;submit.disabled=false;}
  };
  form.append(label,message,note,submit,status,cancel);root.append(open,form);return root;
}
