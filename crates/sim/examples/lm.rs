fn main(){ let l=crabsim::Level::test_level(); for p in l.landmarks(){ println!("{:?} | {} | {:?}", p.kind,p.name,p.rect);} for f in &l.foes { println!("foe {:?} {:?}",f.0,f.1);} }
