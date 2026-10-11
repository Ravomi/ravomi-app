import plus from '../../assets/icons/plus.svg';
import folder from '../../assets/icons/folder-open.svg';
import save from '../../assets/icons/save.svg';
import undo from '../../assets/icons/undo.svg';
import redo from '../../assets/icons/redo.svg';
import moon from '../../assets/icons/moon.svg';
import grid from '../../assets/icons/blocks.svg';
import text from '../../assets/icons/type.svg';
import file from '../../assets/icons/file.svg';
import info from '../../assets/icons/info.svg';

const icons = { plus, folder, save, undo, redo, moon, grid, text, file, info };

/** Licensed Lucide assets shared with the native canvas. */
export function Icon({ name }: { name: keyof typeof icons }) {
  return <span className="icon" aria-hidden="true" style={{ maskImage: `url("${icons[name]}")` }} />;
}
