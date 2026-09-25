import type { ImageSetCollection } from '../imageset/model';
import { Icon } from '../ui/Icon';

export interface ImageSetInspectorProps {
  collection: ImageSetCollection | null;
  onReorder: (memberId: string, targetIndex: number) => void;
  onAlignmentChange: (referenceMember: string | null) => void;
}

function value(value: string | number | null): string {
  return value === null || value === '' ? '—' : String(value);
}

export function ImageSetInspector({ collection, onReorder, onAlignmentChange }: ImageSetInspectorProps) {
  if (!collection) {
    return (
      <section aria-label="ImageSet inspector" className="imageset-inspector imageset-inspector--empty">
        <span className="eyebrow">ImageSet inspector</span>
        <p>Select an ImageSet to inspect its members, order, and alignment reference.</p>
      </section>
    );
  }

  const referenceMember = collection.alignment.state === 'aligned' ? collection.alignment.referenceMember : '';
  return (
    <section aria-label="ImageSet inspector" className="imageset-inspector">
      <div className="imageset-inspector__heading">
        <div>
          <span className="eyebrow">ImageSet inspector</span>
          <h3>{collection.name}</h3>
        </div>
        <span className="imageset-inspector__count">{collection.members.length} members</span>
      </div>
      <dl className="imageset-inspector__summary">
        <div><dt>Semantics</dt><dd>{collection.order}</dd></div>
        <div><dt>Alignment</dt><dd>{collection.alignment.state}</dd></div>
        <div><dt>Camera</dt><dd>{value(collection.sharedMetadata.camera)}</dd></div>
        <div><dt>ISO</dt><dd>{value(collection.sharedMetadata.iso)}</dd></div>
      </dl>
      <label className="imageset-inspector__alignment">
        <span>Alignment reference</span>
        <select
          aria-label="Alignment reference"
          onChange={(event) => onAlignmentChange(event.target.value || null)}
          value={referenceMember}
        >
          <option value="">Unaligned</option>
          {collection.members.map((member) => (
            <option key={member.id} value={member.id}>{member.name}</option>
          ))}
        </select>
      </label>
      <div className="imageset-member-list" aria-label="ImageSet members">
        {collection.members.map((member, index) => (
          <article className="imageset-member" key={member.id}>
            {member.thumbnail ? <img alt="" src={member.thumbnail} /> : <span className="imageset-member__placeholder">◌</span>}
            <div className="imageset-member__body">
              <strong>{member.name}</strong>
              <code title={member.path}>{member.path}</code>
              {member.metadata && <small>{member.metadata.width} × {member.metadata.height} · {value(member.metadata.camera)}</small>}
              {member.error && <p className="imageset-member__error" role="alert">{member.error}</p>}
            </div>
            <div className="imageset-member__actions">
              <button
                aria-label={`Move ${member.name} up`}
                className="icon-button"
                disabled={collection.order === 'unordered' || index === 0}
                onClick={() => onReorder(member.id, index - 1)}
                type="button"
              ><Icon name="arrowUp" /></button>
              <button
                aria-label={`Move ${member.name} down`}
                className="icon-button"
                disabled={collection.order === 'unordered' || index === collection.members.length - 1}
                onClick={() => onReorder(member.id, index + 1)}
                type="button"
              ><Icon name="arrowDown" /></button>
            </div>
          </article>
        ))}
      </div>
      {collection.order === 'unordered' && <p className="empty-state empty-state--compact">Unordered members are kept in canonical path order.</p>}
    </section>
  );
}
